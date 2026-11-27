// Riferimento: decodifica greedy con llama.cpp, un token alla volta, stampa gli id.
#include "llama.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>

#include "ggml-backend.h"
static bool dump_cb(struct ggml_tensor * t, bool ask, void *) {
    const char * want = getenv("DUMPT");
    if (!want) return false;
    if (ask) return strcmp(t->name, want) == 0;
    if (strcmp(t->name, want) == 0 && t->type == GGML_TYPE_F32) {
        static int seen = 0; if (seen++) return true;
        std::vector<float> buf(ggml_nelements(t));
        ggml_backend_tensor_get(t, buf.data(), 0, ggml_nbytes(t));
        FILE * f = fopen("/tmp/rt.txt", "w");
        for (float v : buf) fprintf(f, "%.7f\n", v);
        fclose(f);
    }
    return true;
}
int main(int argc, char ** argv) {
    if (argc < 4) { fprintf(stderr, "uso: ref model.gguf \"prompt\" n\n"); return 1; }
    const char * path = argv[1];
    const char * prompt = argv[2];
    int n_gen = atoi(argv[3]);
    if (!getenv("REFLOG")) llama_log_set([](ggml_log_level, const char *, void *) {}, nullptr);
    ggml_backend_load_all();
    llama_model_params mp = llama_model_default_params();
    mp.n_gpu_layers = 0;
    llama_model * model = llama_model_load_from_file(path, mp);
    if (!model) { fprintf(stderr, "load fallito\n"); return 1; }
    const llama_vocab * vocab = llama_model_get_vocab(model);
    std::vector<llama_token> toks(4096);
    int n = llama_tokenize(vocab, prompt, strlen(prompt), toks.data(), toks.size(), true, false);
    toks.resize(n);
    llama_context_params cp = llama_context_default_params();
    cp.n_ctx = 512; cp.n_batch = 512; cp.n_threads = 1; cp.n_threads_batch = 1;
    cp.type_k = GGML_TYPE_F32; cp.type_v = GGML_TYPE_F32;
    cp.flash_attn_type = LLAMA_FLASH_ATTN_TYPE_DISABLED;
    cp.cb_eval = dump_cb; cp.cb_eval_user_data = nullptr;
    llama_context * ctx = llama_init_from_model(model, cp);
    printf("prompt:");
    for (auto t : toks) printf(" %d", t);
    printf("\ngen:");
    llama_batch b = llama_batch_get_one(toks.data(), toks.size());
    if (getenv("REFSEQ")) {
        for (size_t k = 0; k + 1 < toks.size(); k++) { llama_batch bb = llama_batch_get_one(&toks[k], 1); if (llama_decode(ctx, bb)) return 1; }
        b = llama_batch_get_one(&toks[toks.size()-1], 1);
    }
    for (int i = 0; i < n_gen; i++) {
        if (llama_decode(ctx, b)) { fprintf(stderr, "decode fallito\n"); return 1; }
        const float * lg = llama_get_logits_ith(ctx, -1);
        int nv = llama_vocab_n_tokens(vocab);
        if (i==0 && getenv("DUMP")) { FILE*f=fopen(getenv("DUMP"),"w"); for (int j=0;j<llama_vocab_n_tokens(vocab);j++) fprintf(f,"%.7f\n",lg[j]); fclose(f);}
        int best = 0;
        for (int j = 1; j < nv; j++) if (lg[j] > lg[best]) best = j;
        if (llama_vocab_is_eog(vocab, best)) break;
        if (getenv("REFGAP")) { float s2=-1e30; for (int j=0;j<nv;j++) if (j!=best && lg[j]>s2) s2=lg[j]; fprintf(stderr, "%d:%.5f ", best, lg[best]-s2); }
        printf(" %d", best);
        static llama_token t; t = best;
        b = llama_batch_get_one(&t, 1);
    }
    printf("\n");
    llama_free(ctx);
    llama_model_free(model);
    return 0;
}
