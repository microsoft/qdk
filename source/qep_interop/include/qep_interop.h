#ifndef RUSTLIB_H
#define RUSTLIB_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Initializes backend (see README for possible values).
   Must be called before calling any quantum instructions.
   If called again, old backend will be discarded and replaced with new one. */
void init(const char *backend);

/* Saves profiler output to JSON at given path. 
   Does nothing if backend is not profiler. */
void save_profile(const char *path);

/* Quantum instructions not returning result. Rotation angles are in radians. */
void ccx(size_t control0, size_t control1, size_t target);
void cx(size_t control, size_t target);
void cy(size_t control, size_t target);
void cz(size_t control, size_t target);
void rx(size_t qubit_id, double angle);
void rxx(size_t qubit0, size_t qubit1, double angle);
void ry(size_t qubit_id, double angle);
void ryy(size_t qubit0, size_t qubit1, double angle);
void rz(size_t qubit_id, double angle);
void rzz(size_t qubit0, size_t qubit1, double angle);
void h(size_t qubit_id);
void s(size_t qubit_id);
void sadj(size_t qubit_id);
void sx(size_t qubit_id);
void t(size_t qubit_id);
void tadj(size_t qubit_id);
void x(size_t qubit_id);
void y(size_t qubit_id);
void z(size_t qubit_id);
void swap(size_t qubit0, size_t qubit1);
void reset(size_t qubit_id);
void mz_store(size_t qubit_id, size_t result_id);
void mz_store_reset(size_t qubit_id, size_t result_id);

/* Quantum instructions returning a result. */
size_t read_result(size_t result_id);
size_t mz(size_t qubit_id);
size_t mz_reset(size_t qubit_id);

#ifdef __cplusplus
}
#endif

#endif