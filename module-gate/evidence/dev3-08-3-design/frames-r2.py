# Independent recomputation for dev3-row-overflow.md 2판 (f6fbcf9).
XHG, XSG = 3776, 2176
P, INIT, ALLOC_PATCH = 16344, 16448, 64 + 96
EDGE, COUNT, ITEM = 128, 96, 208
ROOT_N, JOB_N = 128 + 512 + 512, 128 + 1024 + 1024
def nat(b, a): return 128 + b + a
C = 244
W  = XHG + ROOT_N + JOB_N + nat(96, 96) + nat(0, 128) + 0*ITEM + 2*P + ALLOC_PATCH + INIT
W1 = W + ITEM
OC = lambda b, a: XHG + ROOT_N + JOB_N + nat(b, a) + ITEM + 3*P
CR = lambda b, a: XSG + ROOT_N + JOB_N + nat(b, a) + 3*P
bmax, amax = 16 + 40*54, 16 + 40*55
F  = lambda h: 91304 + C + 128*h
CLR = lambda h: 91096 + C + 128*h
PR = lambda h: 89496 + 2*C + 128*h
R, RES = 55528, 57336
X = CR(bmax, amax)
print("W(I=0)", W, "W(I=1)", W1, "9W", 9*W)
print("OverflowCancel base", OC(0,0), "max", OC(bmax, amax))
print("ChainRetire base", CR(0,0), "max", X)
print("F", F(0), F(1), "CLR", CLR(0), CLR(1), "Prune", PR(0), PR(1))
print("Job cleanup", PR(0) + 4*R, "inline formula 311608+2C", 311608 + 2*C)
print("4R", 4*R, "4R+X", 4*R + X, "returned", PR(0) + 4*R - (4*R + X))
k9 = RES + 9*W + F(1) + PR(1) + 4*R
print("k=9 commit path h=1", k9, "MB", k9/1e6, "MiB", k9/2**20)
print("OC within F", OC(bmax, amax) <= F(0))
print("CLR+anchor (alt 12)", CLR(1) + nat(bmax, amax) - 128 + 128 + P, "two-chunk", W + nat(0,128) + INIT)
