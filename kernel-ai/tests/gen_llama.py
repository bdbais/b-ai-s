#!/usr/bin/env python3
"""Genera un GGUF llama minuscolo con pesi casuali (seme fisso) per il collaudo del motore."""
import sys
import numpy as np
import gguf

out = sys.argv[1]
qtype = sys.argv[2] if len(sys.argv) > 2 else "f32"   # f32 | q8_0
moe = len(sys.argv) > 3 and sys.argv[3] == "moe"

rng = np.random.default_rng(1234)

chars = list("abcdefghijklmnopqrstuvwxyz.,!?'")
tokens = ["<unk>", "<s>", "</s>", "▁"] + chars + ["▁" + c for c in chars] + ["th", "he", "▁th", "in", "er", "an"]
scores = [0.0, 0.0, 0.0, -1.0] + [-2.0] * len(chars) + [-1.5] * len(chars) + [-0.5, -0.6, -0.4, -0.7, -0.8, -0.9]
types = [2, 3, 3] + [1] * (len(tokens) - 3)
for b in range(256):
    tokens.append("<0x%02X>" % b); scores.append(0.0); types.append(6)
n_vocab = len(tokens)

n_embd, n_head, n_head_kv, n_layer, n_ff, n_ctx = 64, 4, 2, 3, 96, 256
head = n_embd // n_head
n_expert, n_expert_used = 4, 2

w = gguf.GGUFWriter(out, "llama")
w.add_name("bais-test-llama" + ("-moe" if moe else ""))
w.add_context_length(n_ctx)
w.add_embedding_length(n_embd)
w.add_block_count(n_layer)
w.add_feed_forward_length(n_ff)
w.add_head_count(n_head)
w.add_head_count_kv(n_head_kv)
w.add_rope_dimension_count(head)
w.add_rope_freq_base(10000.0)
w.add_layer_norm_rms_eps(1e-5)
w.add_file_type(gguf.LlamaFileType.ALL_F32 if qtype == "f32" else gguf.LlamaFileType.MOSTLY_Q8_0)
if moe:
    w.add_expert_count(n_expert)
    w.add_expert_used_count(n_expert_used)
w.add_tokenizer_model("llama")
w.add_token_list(tokens)
w.add_token_scores(scores)
w.add_token_types(types)
w.add_bos_token_id(1)
w.add_eos_token_id(2)
w.add_unk_token_id(0)
w.add_add_bos_token(True)
w.add_add_eos_token(False)


def mat(rows, cols, std):
    return (rng.standard_normal((rows, cols)) * std).astype(np.float32)


def put(name, arr, quant=True):
    if quant and qtype == "q8_0" and arr.ndim >= 2 and arr.shape[-1] % 32 == 0:
        data = gguf.quants.quantize(arr, gguf.GGMLQuantizationType.Q8_0)
        w.add_tensor(name, data, raw_dtype=gguf.GGMLQuantizationType.Q8_0)
    else:
        w.add_tensor(name, arr)


s = 1.0 / np.sqrt(n_embd)
put("token_embd.weight", mat(n_vocab, n_embd, 1.0))
for i in range(n_layer):
    p = f"blk.{i}."
    put(p + "attn_norm.weight", (1 + 0.1 * rng.standard_normal(n_embd)).astype(np.float32), False)
    put(p + "attn_q.weight", mat(n_embd, n_embd, s * 2))
    put(p + "attn_k.weight", mat(n_head_kv * head, n_embd, s * 2))
    put(p + "attn_v.weight", mat(n_head_kv * head, n_embd, s))
    put(p + "attn_output.weight", mat(n_embd, n_embd, s))
    put(p + "ffn_norm.weight", (1 + 0.1 * rng.standard_normal(n_embd)).astype(np.float32), False)
    if moe:
        put(p + "ffn_gate_inp.weight", mat(n_expert, n_embd, s * 3), False)
        put(p + "ffn_gate_exps.weight", (rng.standard_normal((n_expert, n_ff, n_embd)) * s).astype(np.float32))
        put(p + "ffn_up_exps.weight", (rng.standard_normal((n_expert, n_ff, n_embd)) * s).astype(np.float32))
        put(p + "ffn_down_exps.weight", (rng.standard_normal((n_expert, n_embd, n_ff)) * (1 / np.sqrt(n_ff))).astype(np.float32))
    else:
        put(p + "ffn_gate.weight", mat(n_ff, n_embd, s))
        put(p + "ffn_up.weight", mat(n_ff, n_embd, s))
        put(p + "ffn_down.weight", mat(n_embd, n_ff, 1 / np.sqrt(n_ff)))
put("output_norm.weight", np.ones(n_embd, dtype=np.float32), False)
put("output.weight", mat(n_vocab, n_embd, 3.0 * s))

w.write_header_to_file()
w.write_kv_data_to_file()
w.write_tensors_to_file()
w.close()
print("scritto", out, n_vocab, "token")
