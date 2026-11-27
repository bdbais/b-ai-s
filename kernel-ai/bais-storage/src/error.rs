/// Errori del livello di memoria di massa. Piccoli e copiabili, così passano
/// anche attraverso l'interfaccia C dell'applicazione UEFI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Error {
    /// Il driver non è riuscito a leggere (codice del driver perso di proposito:
    /// il chiamante lo registra prima di restituirlo).
    Io = -1,
    /// Lettura fuori dal dispositivo o dalla partizione.
    OutOfRange = -2,
    /// Dimensione di blocco non supportata (deve essere 512..4096 e potenza di 2).
    BadBlockSize = -3,
    /// Né GPT né MBR.
    NoPartitionTable = -4,
    /// Intestazione o voci GPT con CRC32 sbagliato.
    GptCrc = -5,
    /// Nessun volume exFAT trovato.
    NoExfat = -6,
    /// Settore di avvio exFAT non valido.
    BadBootSector = -7,
    /// Checksum della regione di avvio sbagliato.
    BootChecksum = -8,
    /// Catena dei cluster rotta (cluster libero, difettoso, fuori range o ciclo).
    BadClusterChain = -9,
    /// Insieme di voci di directory non valido (checksum, nome, ordine).
    BadDirEntry = -10,
    /// Percorso non trovato.
    NotFound = -11,
    /// Una parte del percorso non è una cartella.
    NotADirectory = -12,
    /// Si è chiesto di leggere una cartella come file.
    IsADirectory = -13,
    /// Nome troppo lungo o buffer troppo piccolo.
    NameTooLong = -14,
    /// Checksum della tabella delle maiuscole sbagliato.
    UpcaseChecksum = -15,
}

pub type Result<T> = core::result::Result<T, Error>;

impl Error {
    pub fn code(self) -> i32 {
        self as i32
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Error::Io => "errore di lettura del dispositivo",
            Error::OutOfRange => "lettura fuori dal dispositivo",
            Error::BadBlockSize => "dimensione di blocco non supportata",
            Error::NoPartitionTable => "nessuna tabella delle partizioni",
            Error::GptCrc => "GPT danneggiata (CRC32)",
            Error::NoExfat => "nessun volume exFAT",
            Error::BadBootSector => "settore di avvio exFAT non valido",
            Error::BootChecksum => "checksum di avvio exFAT sbagliato",
            Error::BadClusterChain => "catena dei cluster rotta",
            Error::BadDirEntry => "voce di directory non valida",
            Error::NotFound => "file non trovato",
            Error::NotADirectory => "non è una cartella",
            Error::IsADirectory => "è una cartella",
            Error::NameTooLong => "nome troppo lungo",
            Error::UpcaseChecksum => "tabella delle maiuscole danneggiata",
        }
    }
}
