/*
 * B-AI-S — sonda della chiavetta (Fase 4, primo passo verso l'avvio UEFI).
 *
 * Applicazione UEFI (BOOTX64.EFI): per ogni disco intero visto dal firmware
 * (protocollo BlockIo, chiavette USB comprese) chiede a bais-storage, scritto
 * in Rust no_std, di trovare il volume exFAT, elencare la radice e leggere per
 * intero il file di modello calcolandone lo sha256. Se accanto c'è
 * <file>.sha256 lo confronta e scrive VERIFICATO o DIVERSO.
 *
 * Qui il driver USB è quello del firmware: è il modo più rapido per provare il
 * file system sul PC vero. I driver propri (xHCI + mass storage, NVMe) sono il
 * passo successivo della Fase 4 e useranno la stessa interfaccia BlockDevice.
 */
#include <efi.h>
#include <efilib.h>

#define MODEL_PATH "/modelli/inkling-2-mini.gguf"
#define BUF_BYTES (8u << 20)

typedef struct {
    void *ctx;
    UINT32 block_size;
    UINT64 num_blocks;
    INT32 (*read)(void *ctx, UINT64 lba, UINT64 nbytes, UINT8 *buf);
    void (*log)(void *ctx, const UINT8 *text, UINTN len);
    UINT64 (*ticks)(void);
    UINT64 ticks_per_sec;
} CDev;

typedef struct {
    INT32 error;
    UINT64 part_start, part_blocks, file_size, bytes_read, read_requests, elapsed_ms;
    UINT8 sha256_hex[64];
    INT32 verified;
} Report;

extern INT32 bais_sonda(const CDev *dev, const UINT8 *path, UINTN path_len,
                        UINT8 *buf, UINTN buf_len, Report *report);

/* Funzioni di memoria che il codice Rust si aspetta (memcpy e memset sono in libefi). */
void *memmove(void *d, const void *s, UINTN n) {
    UINT8 *a = d; const UINT8 *b = s;
    if (a < b) { while (n--) *a++ = *b++; } else { a += n; b += n; while (n--) *--a = *--b; }
    return d;
}
int memcmp(const void *x, const void *y, UINTN n) {
    const UINT8 *a = x, *b = y;
    for (; n; n--, a++, b++) if (*a != *b) return *a - *b;
    return 0;
}
int bcmp(const void *x, const void *y, UINTN n) { return memcmp(x, y, n); }

typedef struct {
    EFI_BLOCK_IO *bio;
    UINT8 *bounce;
    UINTN bounce_len;
} Disk;

static INT32 disk_read(void *ctx, UINT64 lba, UINT64 nbytes, UINT8 *buf) {
    Disk *d = ctx;
    UINT32 align = d->bio->Media->IoAlign;
    if (align <= 1 || ((UINTN)buf % align) == 0) {
        EFI_STATUS s = uefi_call_wrapper(d->bio->ReadBlocks, 5, d->bio, d->bio->Media->MediaId, lba, nbytes, buf);
        return EFI_ERROR(s) ? -1 : 0;
    }
    /* Buffer non allineato come chiede il controller: passa da un buffer di appoggio. */
    UINT32 bs = d->bio->Media->BlockSize;
    while (nbytes) {
        UINTN n = nbytes < d->bounce_len ? nbytes : d->bounce_len;
        EFI_STATUS s = uefi_call_wrapper(d->bio->ReadBlocks, 5, d->bio, d->bio->Media->MediaId, lba, n, d->bounce);
        if (EFI_ERROR(s)) return -1;
        CopyMem(buf, d->bounce, n);
        buf += n; nbytes -= n; lba += n / bs;
    }
    return 0;
}

/* UTF-8 → UCS-2 per la console del firmware. */
static void con_log(void *ctx, const UINT8 *t, UINTN len) {
    (void)ctx;
    CHAR16 out[520];
    UINTN o = 0, i = 0;
    while (i < len && o < 512) {
        UINT32 c = t[i];
        if (c < 0x80) { i += 1; }
        else if ((c & 0xE0) == 0xC0 && i + 1 < len) { c = ((c & 0x1F) << 6) | (t[i + 1] & 0x3F); i += 2; }
        else if ((c & 0xF0) == 0xE0 && i + 2 < len) { c = ((c & 0x0F) << 12) | ((t[i + 1] & 0x3F) << 6) | (t[i + 2] & 0x3F); i += 3; }
        else { c = '?'; i += 1; while (i < len && (t[i] & 0xC0) == 0x80) i++; }
        out[o++] = (CHAR16)c;
    }
    out[o++] = L'\r'; out[o++] = L'\n'; out[o] = 0;
    Print(L"%s", out);
}

static UINT64 rdtsc(void) {
    UINT32 lo, hi;
    __asm__ volatile("rdtsc" : "=a"(lo), "=d"(hi));
    return ((UINT64)hi << 32) | lo;
}

EFI_STATUS efi_main(EFI_HANDLE image, EFI_SYSTEM_TABLE *st) {
    InitializeLib(image, st);
    uefi_call_wrapper(BS->SetWatchdogTimer, 4, 0, 0, 0, NULL); /* la lettura può durare minuti */

    Print(L"\r\nB-AI-S sonda della chiavetta v0.1 - bais-storage (Rust no_std)\r\n");
    UINT64 t0 = rdtsc();
    uefi_call_wrapper(BS->Stall, 1, 100000);
    UINT64 tps = (rdtsc() - t0) * 10;
    Print(L"TSC: %ld MHz\r\n", tps / 1000000);

    UINTN count = 0;
    EFI_HANDLE *handles = NULL;
    EFI_STATUS s = LibLocateHandle(ByProtocol, &BlockIoProtocol, NULL, &count, &handles);
    if (EFI_ERROR(s)) { Print(L"nessun BlockIo: %r\r\n", s); return s; }

    UINT8 *buf = AllocatePool(BUF_BYTES);
    Disk disk;
    disk.bounce_len = 1u << 20;
    disk.bounce = AllocatePool(disk.bounce_len);
    if (!buf || !disk.bounce) { Print(L"memoria insufficiente\r\n"); return EFI_OUT_OF_RESOURCES; }

    int verified = 0, tried = 0;
    for (UINTN i = 0; i < count; i++) {
        EFI_BLOCK_IO *bio;
        if (EFI_ERROR(uefi_call_wrapper(BS->HandleProtocol, 3, handles[i], &BlockIoProtocol, (void **)&bio))) continue;
        if (bio->Media->LogicalPartition || !bio->Media->MediaPresent) continue;
        Print(L"\r\ndisco %d: %ld blocchi da %d byte%s\r\n", (int)i, bio->Media->LastBlock + 1,
              bio->Media->BlockSize, bio->Media->RemovableMedia ? L", rimovibile" : L"");
        disk.bio = bio;
        CDev dev = { &disk, bio->Media->BlockSize, bio->Media->LastBlock + 1, disk_read, con_log, rdtsc, tps };
        Report r;
        const char *path = MODEL_PATH;
        UINTN pl = 0; while (path[pl]) pl++;
        INT32 e = bais_sonda(&dev, (const UINT8 *)path, pl, buf, BUF_BYTES, &r);
        if (e == -6 /* nessun exFAT */) { Print(L"  nessun volume exFAT\r\n"); continue; }
        tried++;
        if (e != 0) { Print(L"  ESITO: errore %d\r\n", e); continue; }
        CHAR16 hex[65];
        for (int k = 0; k < 64; k++) hex[k] = r.sha256_hex[k];
        hex[64] = 0;
        UINT64 mbs = r.elapsed_ms ? r.bytes_read / 1000 / r.elapsed_ms : 0;
        Print(L"  letti %ld byte in %ld ms (%ld MB/s), %ld richieste\r\n", r.bytes_read, r.elapsed_ms, mbs, r.read_requests);
        Print(L"  sha256 %s\r\n", hex);
        if (r.verified == 1) { Print(L"  ESITO: VERIFICATO\r\n"); verified++; }
        else if (r.verified == 0) Print(L"  ESITO: DIVERSO dall'atteso\r\n");
        else Print(L"  ESITO: letto, manca il file .sha256 per il confronto\r\n");
    }
    Print(L"\r\nFINE: %d volumi exFAT, %d verificati\r\n", tried, verified);
    FreePool(buf);
    FreePool(disk.bounce);
    /* Con OVMF (QEMU) si spegne subito; su un PC vero il risultato resta a video 60 s. */
    BOOLEAN ovmf = st->FirmwareVendor && StrnCmp(st->FirmwareVendor, L"EDK II", 6) == 0;
    if (!ovmf) uefi_call_wrapper(BS->Stall, 1, 60u * 1000000u);
    uefi_call_wrapper(RT->ResetSystem, 4, EfiResetShutdown, EFI_SUCCESS, 0, NULL);
    return EFI_SUCCESS;
}
