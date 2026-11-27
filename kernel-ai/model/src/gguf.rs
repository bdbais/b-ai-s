//! Lettore GGUF senza copie: metadati, tensori e dati restano nel buffer originale
//! (in kernel: il modulo caricato in RAM o i pesi mappati dal disco).

use alloc::string::String;
use alloc::vec::Vec;

use crate::Error;

#[derive(Debug, Clone)]
pub enum Value {
    U(u64),
    I(i64),
    F(f64),
    Bool(bool),
    Str(String),
    Arr(Vec<Value>),
}

impl Value {
    pub fn as_u64(&self) -> Option<u64> {
        match *self {
            Value::U(v) => Some(v),
            Value::I(v) if v >= 0 => Some(v as u64),
            Value::Bool(b) => Some(b as u64),
            _ => None,
        }
    }
    pub fn as_f32(&self) -> Option<f32> {
        match *self {
            Value::F(v) => Some(v as f32),
            Value::U(v) => Some(v as f32),
            Value::I(v) => Some(v as f32),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }
    pub fn as_arr(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(a) => Some(a.as_slice()),
            _ => None,
        }
    }
}

/// Tipi ggml supportati dal motore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    F32,
    F16,
    Q4_0,
    Q8_0,
    IQ4NL,
    Other(u32),
}

impl DType {
    pub fn from_u32(v: u32) -> DType {
        match v {
            0 => DType::F32,
            1 => DType::F16,
            2 => DType::Q4_0,
            8 => DType::Q8_0,
            20 => DType::IQ4NL,
            o => DType::Other(o),
        }
    }
    /// (elementi per blocco, byte per blocco)
    pub fn block(self) -> Option<(usize, usize)> {
        match self {
            DType::F32 => Some((1, 4)),
            DType::F16 => Some((1, 2)),
            DType::Q4_0 => Some((32, 18)),
            DType::Q8_0 => Some((32, 34)),
            DType::IQ4NL => Some((32, 18)),
            DType::Other(_) => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            DType::F32 => "F32",
            DType::F16 => "F16",
            DType::Q4_0 => "Q4_0",
            DType::Q8_0 => "Q8_0",
            DType::IQ4NL => "IQ4_NL",
            DType::Other(_) => "?",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Tensor<'a> {
    /// ne[0] è la dimensione contigua (come in ggml)
    pub ne: [usize; 4],
    pub n_dims: usize,
    pub dtype: DType,
    pub data: &'a [u8],
}

impl<'a> Tensor<'a> {
    pub fn n_elements(&self) -> usize {
        self.ne.iter().product()
    }
    /// byte di una riga di ne[0] elementi
    pub fn row_bytes(&self) -> usize {
        let (be, bb) = self.dtype.block().unwrap_or((1, 0));
        self.ne[0] / be * bb
    }
    pub fn n_rows(&self) -> usize {
        self.ne[1] * self.ne[2] * self.ne[3]
    }
    pub fn row(&self, r: usize) -> &'a [u8] {
        let rb = self.row_bytes();
        &self.data[r * rb..(r + 1) * rb]
    }
    /// sotto-matrice `i` lungo ne[2] (gli esperti di un tensore MoE)
    pub fn slice2(&self, i: usize) -> Tensor<'a> {
        let per = self.row_bytes() * self.ne[1];
        Tensor {
            ne: [self.ne[0], self.ne[1], 1, 1],
            n_dims: 2,
            dtype: self.dtype,
            data: &self.data[i * per..(i + 1) * per],
        }
    }
}

pub struct TensorInfo {
    pub name: String,
    pub ne: [usize; 4],
    pub n_dims: usize,
    pub dtype: DType,
    pub offset: usize,
}

pub struct Gguf<'a> {
    pub version: u32,
    pub kv: Vec<(String, Value)>,
    pub tensors: Vec<TensorInfo>,
    pub data: &'a [u8],
}

struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        if self.p + n > self.b.len() {
            return Err(Error::Format("GGUF troncato"));
        }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, Error> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Result<u32, Error> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn u64(&mut self) -> Result<u64, Error> {
        let s = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        Ok(u64::from_le_bytes(a))
    }
    fn string(&mut self) -> Result<String, Error> {
        let n = self.u64()? as usize;
        let s = self.take(n)?;
        Ok(String::from(core::str::from_utf8(s).map_err(|_| Error::Format("stringa non UTF-8"))?))
    }
    fn value(&mut self, t: u32) -> Result<Value, Error> {
        Ok(match t {
            0 => Value::U(self.u8()? as u64),
            1 => Value::I(self.u8()? as i8 as i64),
            2 => Value::U(self.u16()? as u64),
            3 => Value::I(self.u16()? as i16 as i64),
            4 => Value::U(self.u32()? as u64),
            5 => Value::I(self.u32()? as i32 as i64),
            6 => Value::F(f32::from_bits(self.u32()?) as f64),
            7 => Value::Bool(self.u8()? != 0),
            8 => Value::Str(self.string()?),
            9 => {
                let et = self.u32()?;
                let n = self.u64()? as usize;
                let mut v = Vec::with_capacity(n.min(1 << 20));
                for _ in 0..n {
                    v.push(self.value(et)?);
                }
                Value::Arr(v)
            }
            10 => Value::U(self.u64()?),
            11 => Value::I(self.u64()? as i64),
            12 => Value::F(f64::from_bits(self.u64()?)),
            _ => return Err(Error::Format("tipo di valore GGUF sconosciuto")),
        })
    }
}

impl<'a> Gguf<'a> {
    pub fn parse(b: &'a [u8]) -> Result<Gguf<'a>, Error> {
        let mut r = Rd { b, p: 0 };
        if r.take(4)? != b"GGUF" {
            return Err(Error::Format("non è un file GGUF"));
        }
        let version = r.u32()?;
        if version < 2 {
            return Err(Error::Format("versione GGUF non supportata"));
        }
        let n_t = r.u64()? as usize;
        let n_kv = r.u64()? as usize;
        let mut kv = Vec::with_capacity(n_kv);
        for _ in 0..n_kv {
            let k = r.string()?;
            let t = r.u32()?;
            let v = r.value(t)?;
            kv.push((k, v));
        }
        let mut tensors = Vec::with_capacity(n_t);
        for _ in 0..n_t {
            let name = r.string()?;
            let n_dims = r.u32()? as usize;
            if n_dims > 4 {
                return Err(Error::Format("tensore con più di 4 dimensioni"));
            }
            let mut ne = [1usize; 4];
            for d in ne.iter_mut().take(n_dims) {
                *d = r.u64()? as usize;
            }
            let dtype = DType::from_u32(r.u32()?);
            let offset = r.u64()? as usize;
            tensors.push(TensorInfo { name, ne, n_dims, dtype, offset });
        }
        let mut g = Gguf { version, kv, tensors, data: &[] };
        let align = g.get("general.alignment").and_then(|v| v.as_u64()).unwrap_or(32) as usize;
        let start = (r.p + align - 1) / align * align;
        if start > b.len() {
            return Err(Error::Format("GGUF troncato"));
        }
        g.data = &b[start..];
        Ok(g)
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.kv.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn arch(&self) -> &str {
        self.get("general.architecture").and_then(|v| v.as_str()).unwrap_or("?")
    }

    /// chiave specifica dell'architettura, es. "llama.block_count"
    pub fn get_arch(&self, key: &str) -> Option<&Value> {
        let mut k = String::from(self.arch());
        k.push('.');
        k.push_str(key);
        self.get(&k)
    }

    pub fn u(&self, key: &str) -> Result<usize, Error> {
        self.get_arch(key).and_then(|v| v.as_u64()).map(|v| v as usize).ok_or(Error::Missing(String::from(key)))
    }

    pub fn f(&self, key: &str) -> Result<f32, Error> {
        self.get_arch(key).and_then(|v| v.as_f32()).ok_or(Error::Missing(String::from(key)))
    }

    pub fn tensor(&self, name: &str) -> Result<Tensor<'a>, Error> {
        self.try_tensor(name).ok_or_else(|| Error::Missing(String::from(name)))
    }

    pub fn try_tensor(&self, name: &str) -> Option<Tensor<'a>> {
        let t = self.tensors.iter().find(|t| t.name == name)?;
        let (be, bb) = t.dtype.block()?;
        let n: usize = t.ne.iter().product();
        let bytes = n / be * bb;
        if t.offset + bytes > self.data.len() {
            return None;
        }
        Some(Tensor { ne: t.ne, n_dims: t.n_dims, dtype: t.dtype, data: &self.data[t.offset..t.offset + bytes] })
    }
}
