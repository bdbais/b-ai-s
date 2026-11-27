#!/usr/bin/env python3
"""Inkling 2 Mini in miniatura (pesi casuali, seme fisso): stessa architettura `inkling2_loop`
dell'originale (blocchi, nucleo ripetuto, SWA + attenzione piena, MoE sigmoide, convoluzioni
brevi, controllori del loop), dimensioni minuscole. Serve a confrontare il motore con llama.cpp."""
import sys
import numpy as np
import gguf

out = sys.argv[1]
loop = len(sys.argv) <= 2 or sys.argv[2] != "noloop"
qtype = sys.argv[3] if len(sys.argv) > 3 else "f32"
arch = "inkling2_loop" if loop else "inkling2"
rng = np.random.default_rng(4321)


def bytes_to_unicode():
    bs = list(range(ord("!"), ord("~") + 1)) + list(range(ord("¡"), ord("¬") + 1)) + list(range(ord("®"), ord("ÿ") + 1))
    cs = bs[:]
    n = 0
    for b in range(256):
        if b not in bs:
            bs.append(b)
            cs.append(256 + n)
            n += 1
    return dict(zip(bs, [chr(c) for c in cs]))


b2u = bytes_to_unicode()
tokens = [b2u[b] for b in range(256)]
merges = ["h e", "l l", "t h", "i n", "in g", "Ġ t", "Ġt he", "Ġ c", "Ġc a", "Ġca t", "e r", "a n", "Ġ w", "o r", "Ġw or"]
for m in merges:
    a, b = m.split(" ")
    if a + b not in tokens:
        tokens.append(a + b)
for w in ["hello", "Ġworld", "Ġbeginning", "Ġthere"]:
    tokens.append(w)
tokens += ["<|end|>", "<|pad|>"]
types = [1] * (len(tokens) - 2) + [3, 3]
n_vocab = len(tokens)
n_vocab_padded = (n_vocab + 63) // 64 * 64

E, H, HKV, HD = 64, 4, 2, 16
n_layer, block = 6, 2
core_start, core_end, n_pass, rank = 2, 4, 3, 8
n_expert, n_used, ff = 8, 2, 32
n_rel, rel_extent, n_swa, n_conv = 8, 64, 8, 4
swa = [True, False, True, False, True, False]

w = gguf.GGUFWriter(out, arch)
w.add_name("bais-test-inkling2" + ("-loop" if loop else ""))
w.add_context_length(512)
w.add_embedding_length(E)
w.add_block_count(n_layer)
w.add_feed_forward_length(ff)
w.add_head_count(H)
w.add_head_count_kv(HKV)
w.add_key_length(HD)
w.add_value_length(HD)
w.add_layer_norm_rms_eps(1e-5)
w.add_expert_count(n_expert)
w.add_expert_used_count(n_used)
w.add_uint32(f"{arch}.expert_feed_forward_length", ff)
w.add_float32(f"{arch}.attention.qk_clip", 4.0)
w.add_uint32(f"{arch}.attn_res.block_size", block)
w.add_uint32(f"{arch}.shortconv.l_cache", n_conv)
w.add_uint32(f"{arch}.attention.relative_length", n_rel)
w.add_uint32(f"{arch}.attention.relative_extent", rel_extent)
w.add_float32(f"{arch}.attention.relative_clip", 3.0)
w.add_uint32(f"{arch}.attention.log_scale_floor", 4)
w.add_float32(f"{arch}.attention.log_scale_alpha", 0.5)
w.add_uint32(f"{arch}.vocab_size", n_vocab_padded)
w.add_uint32(f"{arch}.vocab_size_unpadded", n_vocab)
w.add_float32(f"{arch}.activation.situ_beta", 3.0)
w.add_float32(f"{arch}.activation.situ_linear_beta", 4.0)
w.add_float32(f"{arch}.expert_weights_scale", 1.5)
w.add_uint32(f"{arch}.attention.sliding_window", n_swa)
w.add_array(f"{arch}.attention.sliding_window_pattern", swa)
if loop:
    w.add_uint32(f"{arch}.loop.start", core_start)
    w.add_uint32(f"{arch}.loop.end", core_end)
    w.add_uint32(f"{arch}.loop.count", n_pass)
    w.add_uint32(f"{arch}.loop.gate_rank", rank)
w.add_tokenizer_model("gpt2")
w.add_tokenizer_pre("inkling2")
w.add_token_list(tokens)
w.add_token_types(types)
w.add_token_merges(merges)
w.add_eos_token_id(n_vocab - 2)
w.add_pad_token_id(n_vocab - 1)
w.add_add_bos_token(False)


def mat(*shape, std=1.0):
    return (rng.standard_normal(shape) * std).astype(np.float32)


def gain(n):
    return (1 + 0.2 * rng.standard_normal(n)).astype(np.float32)


def put(name, arr, quant=True):
    if quant and qtype == "q8_0" and arr.ndim >= 2 and arr.shape[-1] % 32 == 0:
        data = gguf.quants.quantize(arr, gguf.GGMLQuantizationType.Q8_0)
        w.add_tensor(name, data, raw_dtype=gguf.GGMLQuantizationType.Q8_0)
    else:
        w.add_tensor(name, arr)


s = 1 / np.sqrt(E)
put("token_embd.weight", mat(n_vocab_padded, E))
put("token_embd_norm.weight", gain(E), False)
put("output.weight", mat(n_vocab_padded, E, std=3 * s))
put("output_norm.weight", gain(E), False)
put("output_res_query.weight", mat(E, std=0.5), False)
for i in range(n_layer):
    p = f"blk.{i}."
    ext = n_swa if swa[i] else rel_extent
    put(p + "attn_q.weight", mat(H * HD, E, std=s))
    put(p + "attn_k.weight", mat(HKV * HD, E, std=s))
    put(p + "attn_v.weight", mat(HKV * HD, E, std=s))
    put(p + "attn_output.weight", mat(E, H * HD, std=1 / np.sqrt(H * HD)))
    put(p + "attn_gate.weight", mat(H * HD, E, std=s))
    put(p + "attn_q_clip.weight", gain(H * HD).reshape(H, HD), False)
    put(p + "attn_sink_proj.weight", mat(H, E, std=s), False)
    put(p + "attn_rel.weight", mat(n_rel * H, E, std=s))
    put(p + "attn_rel_bias.weight", mat(ext, n_rel, std=0.5), False)
    put(p + "attn_rel_clip.weight", gain(H * n_rel).reshape(H, n_rel), False)
    put(p + "attn_k_conv.weight", mat(HKV * HD, n_conv, std=0.4), False)
    put(p + "attn_v_conv.weight", mat(HKV * HD, n_conv, std=0.4), False)
    put(p + "attn_post_conv.weight", mat(E, n_conv, std=0.4), False)
    put(p + "ffn_post_conv.weight", mat(E, n_conv, std=0.4), False)
    if swa[i]:
        put(p + "attn_norm.weight", gain(E), False)
    else:
        put(p + "attn_q_inp_norm.weight", gain(E), False)
        put(p + "attn_k_inp_norm.weight", gain(E), False)
        put(p + "attn_v_inp_norm.weight", gain(E), False)
        put(p + "attn_k_conv_proj.weight", mat(n_conv * HKV, E, std=s))
        put(p + "attn_v_conv_proj.weight", mat(n_conv * HKV, E, std=s))
    if i > 0:
        put(p + "attn_res_query.weight", mat(1 if swa[i] else 3, E, std=0.5), False)
    put(p + "ffn_res_query.weight", mat(E, std=0.5), False)
    put(p + "ffn_norm.weight", gain(E), False)
    put(p + "ffn_gate_inp.weight", mat(n_expert, E, std=2 * s), False)
    put(p + "exp_probs_b.bias", mat(n_expert, std=0.3), False)
    put(p + "ffn_gate_up_exps.weight", mat(n_expert, 2 * ff, E, std=2 * s))
    put(p + "ffn_down_exps.weight", mat(n_expert, E, ff, std=1 / np.sqrt(ff)))
    if loop and core_start <= i < core_end:
        put(p + "loop_msg_query.weight", mat(4, E, std=0.5), False)
        for c in "qkv":
            put(p + f"loop_{c}_ctrl_down.weight", mat(rank, 2 * E, std=1 / np.sqrt(2 * E)), False)
            put(p + f"loop_{c}_ctrl_gain.weight", mat(E, rank, std=0.5), False)
            put(p + f"loop_{c}_ctrl_forget.weight", mat(E, rank, std=0.5), False)
        put(p + "loop_ffn_ctrl_down.weight", mat(rank, 2 * E, std=1 / np.sqrt(2 * E)), False)
        put(p + "loop_ffn_ctrl_gain.weight", mat(E, rank, std=0.5), False)
        put(p + "loop_ffn_ctrl_forget.weight", mat(E, rank, std=0.5), False)

w.write_header_to_file()
w.write_kv_data_to_file()
w.write_tensors_to_file()
w.close()
print("scritto", out, arch, n_vocab, "token")
