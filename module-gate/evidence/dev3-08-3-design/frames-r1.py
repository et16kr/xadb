# Recompute frame sizes from operation.rs formulas.
P = 16344            # whole-body Patch redo (64 + 16280)
INIT = 16448         # 64 + 16384
ALLOC_PATCH = 64 + 96
def native(before, after): return 128 + before + after
ROOT = native(512, 512); JOB = native(1024, 1024); U = native(432, 432); STATUS = native(224, 224)
def frame(handoff, natives, edges, counts, items, redo):
    return (3776 if handoff else 2176) + natives + edges*128 + counts*96 + items*208 + redo
print("ROOT JOB U STATUS", ROOT, JOB, U, STATUS)
reserve = frame(True, ROOT+JOB+U, 0,0,1, 3*P); print("Reserve", reserve)
cancel = frame(True, ROOT+JOB, 0,0,1, 2*P); print("Cancel", cancel)
release = frame(False, ROOT+JOB+U, 0,0,0, 3*P); print("ReleaseRole R", release)
for h in (0,1):
    C=244
    fwd = frame(True, 5024+C, 3+h, 2, 1, 5*P); clr = frame(True, 5024+C, 3+h,2,0,5*P); prune = frame(False, 5024+2*C, 3+h,2,0,5*P)
    print(f"h={h} Forward(C=244)={fwd} CLR={clr} Prune={prune}")
chunk_native = native(0, 128)   # presence 2 after only
alloc_native = native(96, 96)
cw = frame(True, ROOT+JOB+alloc_native+chunk_native, 0,0,1, 2*P+ALLOC_PATCH+INIT)
print("alloc native", alloc_native, "chunk native", chunk_native, "ChunkWrite", cw)
oc0 = frame(True, ROOT+JOB+128, 0,0,1, 3*P); print("OverflowCancel base (b=a=0)", oc0)
cr0 = frame(False, ROOT+JOB+128, 0,0,0, 3*P); print("ChainRetire base", cr0)
# b, a: XTR1 cell lengths, before may be empty (0) or 16+40n; after 16+40(n+w)
best_oc = max((0 if n0==0 else 16+40*n0) + 16+40*(n0+w) for w in range(1,10) for n0 in range(0,56) if n0+w<=55)
print("max b+a", best_oc, "OC max", oc0+best_oc, "CR max", cr0+best_oc)
print("doc OC formula const 56568 + max", 56568+best_oc)
R = release
X = cr0 + best_oc
print("4R", 4*R, "4R+X", 4*R+X)
for h in (0,1):
    prune = frame(False, 5024+2*244, 3+h,2,0,5*P)
    print(f"h={h} prune path {prune+4*R} CLR path {X+4*R} diff {prune-X}")
print("inline cleanup const", 89496 + 4*R)
W = cw
for h in (1,):
    F = frame(True, 5024+244, 3+h, 2, 1, 5*P)
    prune = frame(False, 5024+2*244, 3+h,2,0,5*P)
    tot = reserve + 9*W + F + prune + 4*R
    print("k=9 commit path", reserve, 9*W, F, prune, 4*R, "total", tot)
# chunk count
import math
for L in (2049,16092,16093,32184,32185,131072):
    print(L, math.ceil(L/16092))
# CLR + anchor alternative
clr1 = frame(True, 5024+244, 4, 2, 0, 5*P)
print("CLR+anchor native+P (k=9 entries)", clr1 + 128 + P + max((0 if n0==0 else 16+40*n0)+16+40*(n0+9) for n0 in range(0,47)))
print("two chunks group", frame(True, ROOT+JOB+alloc_native+2*chunk_native, 0,0,1, 2*P+ALLOC_PATCH+2*INIT))
