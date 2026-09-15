/*
 * DiskDrift core — C ABI for native frontends.
 *
 * Every function returns a heap-allocated, NUL-terminated JSON string
 * (schema version 1) or {"error":"..."} on failure. Free the result with
 * dd_free_string(). NULL string arguments mean "use the default".
 */
#ifndef DISKDRIFT_CDISKDRIFT_H
#define DISKDRIFT_CDISKDRIFT_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* {"version":"0.6.0"} */
char *dd_version(void);

/* Free a string returned by any dd_* function. */
void dd_free_string(char *ptr);

/* {"path","total_bytes","free_bytes","available_bytes","used_bytes"} */
char *dd_disk_usage(const char *path);

/* Scan result (see `diskdrift scan --json`). */
char *dd_scan(const char *home, const char *data_dir, const char *path,
              int32_t depth, int32_t threads);

/* Create a snapshot and return its metadata. */
char *dd_snapshot(const char *home, const char *data_dir, const char *path,
                  int32_t depth, int32_t threads);

/* Deep macOS storage: volumes, local snapshots, VM/swap, system caches. */
char *dd_system(const char *home, int32_t threads);

/* History for all categories (category == NULL) or one category token. */
char *dd_history(const char *home, const char *data_dir, const char *category);

/* Recent watch events: since is a duration like "24h" (NULL for all). */
char *dd_events(const char *home, const char *data_dir, const char *since,
                int32_t limit);

/* Aggregated incidents: since OR from/to (both may be NULL for 24h). */
char *dd_what_happened(const char *home, const char *data_dir,
                       const char *since, const char *from, const char *to,
                       int32_t limit);

#ifdef __cplusplus
}
#endif

#endif /* DISKDRIFT_CDISKDRIFT_H */
