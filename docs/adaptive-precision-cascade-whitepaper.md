# The Adaptive Precision Cascade: A Policy-Driven Planner for Numeric Representation

**With the Universal Numeric Substrate as Its Execution Environment**

**Author:** Jericho Aquino
**Contact:** jericho@skiddph.com
**Affiliation:** Independent Researcher, Puerto Princesa City, Palawan, Philippines

---

## Abstract

We present the **Adaptive Precision Cascade (APC)**: a policy-driven runtime and compile-time planner that treats the choice of numeric representation — which tier of integer, floating-point, rational, decimal, or symbolic arithmetic to use for a given operation — as a single constrained optimization problem, solved jointly across compilation and execution. APC is the central contribution of this work. The **Universal Numeric Substrate (UNS)** is not itself the contribution; it is the representational environment — a chain of numeric tiers with proven value-preservation and capability properties — over which APC operates. We make this distinction explicit throughout, having found in earlier drafts of this work that conflating "a new universal number type" with "a new planning algorithm for selecting among existing number types" understated the actual novelty of the latter while overstating the novelty of the former.

We first argue, on independent grounds, why numeric correctness should be treated as a *dynamic, policy-governed runtime property* rather than a *static, declaration-time property* — a position we defend against four alternative postures (static fixed-width typing, arbitrary precision everywhere, interval-only systems, and symbolic-only systems) before presenting APC as a synthesis rather than a replacement for any of them. We then give a formal model of APC as a state space, a tier lattice, a transition relation, promotion and demotion functions, a policy function, a correctness predicate, a cost function, and a termination condition, and we prove — via proof sketches, explicitly not yet mechanized — that promotion is value-preserving, that demotion is safe if and only if it is representationally justified, that the planner's search always terminates, that error bounds cannot be silently narrowed, and that a simple greedy search solves the planner's optimization problem exactly under two stated monotonicity assumptions. We then describe one reference implementation strategy realizing this model on conventional compiled, multi-threaded hardware without reintroducing the costs the model is meant to eliminate, catalogue known failure modes with their detection and mitigation status, and propose a four-category evaluation framework — correctness, performance, adaptivity, and developer experience — for the empirical work this paper does not itself perform. We close by stating precisely what remains to be proven, built, and measured before APC's claims should be regarded as established rather than proposed.

---

## 1. Introduction

### 1.1 The problem is a design assumption, not a bug list

Almost every well-known category of numeric software failure traces back to one design assumption: **the representation of a number is a property of the variable, decided once, by the programmer, at the point the variable is declared.** Whether a representation is *adequate* is not knowable at declaration time — it is a property of the values a variable will eventually hold and the precision the surrounding logic needs, both of which are typically only known at run time, if at all in advance. This mismatch produces several recurring, structurally distinct classes of defect: **representation error** (values inexact in the chosen base accumulate drift), **range failure** (values exceed a container's bounds and wrap, saturate, or crash), **algorithmic error** (a specific implementation of an operation is wrong for some operand range, independent of storage), **semantic mismatch** (arithmetically valid combination of incompatible units or quantities), and **presentation divergence** (a displayed value diverges from the value actually held). A further, softer problem — **type proliferation** — arises as language designers respond to the first four by adding more specialized numeric types, increasing the burden of choosing correctly rather than removing it.

### 1.2 Central thesis

We state our thesis directly: **numeric representation selection can be formalized as a single constrained optimization problem — minimize evaluation cost subject to soundness, tolerance, and determinism constraints — solved jointly across compile time and run time, and this problem admits an exact greedy solution under two monotonicity assumptions that hold for the representations in common use.** The planner that solves this problem, APC, is the paper's contribution. The tier chain it operates over, UNS, is necessary scaffolding — every planner needs a space of options to plan over — but it is deliberately unambitious as an object in its own right: it does not introduce a new kind of number, only a disciplined way of moving among numbers that already exist.

### 1.3 Structure and epistemic status of claims

§2 defends, on independent philosophical grounds, why this problem should be solved dynamically rather than statically. §3 surveys prior art. §4 gives APC's formal model: state space, tier lattice, transition relation, promotion and demotion functions, policy function, correctness predicate, cost function, and termination condition, followed by proof sketches for the properties that follow from them. §5 describes one reference implementation strategy. §6 states plainly what is and is not novel. §7 catalogues failure modes with their detection and mitigation status. §8 proposes an evaluation framework across four categories. §9 scopes the remaining work. Throughout, we distinguish **theorems** (Layer 1, §4, proof sketches provided) from **design targets** (Layer 2, §5, not yet measured) from **open questions** (§7–§8, not yet answered), and we do not use the language of guarantee or proof outside the first category.

---

## 2. Why Numerical Correctness Should Be Dynamic, Not Static

Before presenting APC's mechanics, we defend the position that numeric representation adequacy is not a decidable static property in general, and that four natural alternatives to a dynamic planner each fail for a structural reason rather than an incidental one.

**Static fixed-width typing** (the status quo in most systems languages) requires the developer to bound, in advance, the magnitude and precision a value will ever need. This is exactly the prediction problem described in §1.1: it is sound when the prediction is right and silently unsound — via wraparound, truncation, or representation error — when it is wrong, with no mechanism to detect the mismatch at the point it occurs. Its failure mode is not a limitation of any particular type; it is that *any* fixed a priori bound is falsifiable by data the type system cannot see.

**Arbitrary precision everywhere** (the common corrective in dynamic languages) removes the prediction problem by removing the bound: every value is unbounded and exact. This is sound but not scalable — it pays the cost of the least efficient tier for every value, including the overwhelming majority that never needed it, which is disqualifying for systems and high-performance code (§1.1). Its failure mode is the mirror image of static typing's: it solves correctness by abandoning the cost model entirely.

**Interval-only systems** (used in numerical verification tooling) track guaranteed bounds on every value without necessarily changing representation, which makes error visible but does not, by itself, decide what to do when an interval grows too wide — a verification tool can report the problem to a human, but a general-purpose language runtime cannot pause and wait for one. Interval tracking is necessary (§4.7 makes it part of APC's correctness predicate) but not sufficient on its own; it needs a response mechanism, not just a measurement.

**Symbolic-only systems** (computer algebra systems) defer all computation to an exact symbolic form and only materialize a numeric approximation on demand. This achieves exactness and defers the cost question indefinitely, but "indefinitely" is itself the problem for a long-running process: an ever-growing expression graph is a resource leak by another name (§7 catalogues this concretely as symbolic-growth failure). Symbolic deferral is a valuable *tier*, not a viable universal strategy.

Each of these four postures makes a fixed, global commitment — to a bound, to a cost, to a measurement without a response, or to a deferral without a limit — at the point where the actual answer depends on data available only at run time, per value, per operation. APC's position is that none of these commitments should be made globally at all: the commitment (which representation, how much deferral, when to escalate) should be made **locally, per operation, informed by the operand's actual magnitude and the policy's actual tolerance**, and revisited automatically when the data outgrows the current commitment. §4 formalizes this as a specific optimization problem rather than a general appeal to "adaptivity," and shows that the four postures above correspond to degenerate special cases of it: static typing is APC restricted to a single tier with no escalation permitted; arbitrary precision everywhere is APC restricted to always start at the top tier; interval-only is APC with the search suppressed and only the correctness predicate retained; symbolic-only is APC restricted to defer materialization indefinitely. APC's synthesis is not a fifth alternative alongside these four; it is the general problem of which they are each a boundary case.

---

## 3. Related Work

APC and UNS compose, rather than invent, the following:

- **Arbitrary-precision (bignum) arithmetic** removes range failure for integers at the cost of speed and memory.
- **Exact rational arithmetic** removes representation error for values reachable by the four basic operations on finite decimal or integer literals.
- **Arbitrary-precision decimal/floating-point libraries** trade speed for reduced representation error without full symbolic computation.
- **Interval arithmetic** and **affine arithmetic** track a guaranteed-containing range rather than a single approximate value, making accumulated error an explicit, queryable quantity.
- **Unit-of-measure and dimensional type systems** attach a dimension vector to numeric values and reject dimensionally incompatible operations.
- **Checked/saturating arithmetic** detects range overflow at the operation level.
- **N-version/redundant computation** re-derives a result via a second, independent method to catch implementation-specific flaws.
- **Tagged pointers and NaN-boxing**, used in high-performance dynamic-language runtimes, compress a value's tag and payload into a single machine word for the common case.
- **Monomorphization**, used by statically compiled generic-programming systems, generates a fully specialized code path per concrete instantiation at compile time.
- **Escape analysis**, used by optimizing compilers and garbage collectors, determines statically whether an allocation's lifetime is provably confined to a lexical or thread scope.
- **Epoch-based reclamation (EBR) and read-copy-update (RCU)**, used in lock-free concurrent data structures, allow read-mostly shared immutable data to be reclaimed without per-access synchronization.
- **Biased/thread-local reference counting**, used by some functional-language runtimes, avoids atomic reference-count traffic for the common single-owner case.
- **Abstract interpretation and interval-based static analysis of floating-point programs**, used in verification tools for safety-critical numeric code, formalize the relationship between a program's floating-point behavior and its idealized real-number semantics.
- **Query planners in database systems**, which choose an execution strategy at run time from cost and cardinality estimates rather than requiring the schema author to fix one in advance — the closest existing analogue to APC's role, applied to a different domain.

To our knowledge, no existing general-purpose language runtime formulates the *choice among* numeric representations as a single constrained optimization problem with a proof of exactness for its solution procedure; each of the above is applied in isolation, by a developer or a tool, to a specific known problem, in the manner a query planner's individual join algorithms exist independently of the planner that chooses among them.

---

## 4. The Formal Model of the Adaptive Precision Cascade

This section defines APC and its environment UNS as mathematical objects, independent of any particular implementation, organized around the components a planner of this kind requires: a state space, the lattice it plans over, a transition relation, promotion and demotion functions, a policy function, a correctness predicate, a cost function, and a termination condition. Everything in this section should remain true regardless of whether it is realized by a compiler emitting native machine code, an interpreter, a GPU kernel, a distributed runtime, or a mechanically checked implementation in a proof assistant.

### 4.1 Semantic domain

Let **V = ℝ ∪ {⊥}** be the domain of *true mathematical values*, where ⊥ denotes an undefined result (division by zero, a domain error). Every numeric expression denotes an element of V; APC's task is to compute a representable approximation of that element together with a certified bound on its distance from it.

### 4.2 The Tier Lattice

**Definition 1 (Tier Chain).** A tier chain is a finite, totally ordered set **𝒯 = ⟨T₀, T₁, …, Tₙ⟩**, T₀ < T₁ < … < Tₙ. Each Tᵢ has a representable set Rᵢ ⊆ V with decoding function decodeᵢ : Rᵢ → V, a cost function costᵢ : Ops → ℝ⁺, and a determinism flag detᵢ ∈ {true, false}.

We use "chain" rather than "lattice" deliberately: the results below require only a total order with monotonic capability and cost (A1–A2), not join/meet structure. A generalization to a true lattice, admitting incomparable specialized tiers, is future work (§9); we retain the name "Cascade" for continuity, but the object defined here is a chain.

- **A1 (Capability Monotonicity).** For i < j, Rᵢ ⊆ Rⱼ, and decodeⱼ restricted to Rᵢ agrees with decodeᵢ.
- **A2 (Cost Monotonicity).** For every op and i < j, costᵢ(op) ≤ costⱼ(op).
- **A3 (Top-Tier Exactness).** Rₙ = V, and evaluation at Tₙ produces a zero-width error interval for any finite, well-defined expression.

### 4.3 State Space

**Definition 2 (Numeric State).** A numeric state is a tuple **σ = (i, p, ε, d, π)**: tier i ∈ 𝒯, payload p ∈ Rᵢ, error interval ε = [lo, hi], dimension vector d (§4.10) or "dimensionless," and optional provenance π (an expression handle used only under deferred evaluation, §5.4). The **state space** Σ is the set of all such tuples ranging over all tiers, and StatesAtTier(i) ⊆ Σ denotes those with first component i.

### 4.4 Correctness Predicate

**Definition 3 (Correctness Predicate).** For a state σ = (i, p, [lo,hi], d, π) and a true value v ∈ V, define `Correct(σ, v) ≡ v − decodeᵢ(p) ∈ [lo, hi]`. A state is **valid** if `Correct(σ, v)` holds for the v it purports to approximate; it is **invalid** otherwise. Every operation defined in this model is required to map valid states to valid states — this is APC's foundational invariant, and every subsequent theorem in §4.11 is, in one form or another, a statement that a specific operation preserves it.

### 4.5 Transition Relation

**Definition 4 (Transition Relation).** Let → ⊆ Σ⁺ × Σ be the relation induced by APC's primitive operations: for an operation op ∈ Ops and input states σ₁,…,σₖ, `(σ₁,…,σₖ) → σ′` holds iff σ′ = apply(op, σ₁,…,σₖ) under some concrete evaluation at some tier. Promotion (§4.6) and demotion (§4.7) are themselves transitions in this relation, of arity one, that change only the tier component while (per Theorem 1) leaving the denoted value fixed. The Cascade's planning problem (§4.9) is, in these terms, the problem of choosing *which* transition — at which tier — to take from a given set of input states, among all transitions the relation admits.

### 4.6 Promotion Function

**Definition 5 (Promotion).** For i < j, `↑ᵢⱼ : StatesAtTier(i) → StatesAtTier(j)`, `↑ᵢⱼ(i, p, ε, d, π) = (j, eᵢⱼ(p), ε, d, π)`, where `eᵢⱼ : Rᵢ → Rⱼ` is the canonical embedding guaranteed by A1, satisfying `decodeⱼ(eᵢⱼ(p)) = decodeᵢ(p)`.

### 4.7 Demotion Function

**Definition 6 (Demotion).** For i < j, `↓ⱼᵢ : StatesAtTier(j) ⇀ StatesAtTier(i)` is *partial*, defined at (j, p, ε, d, π) iff ∃ p′ ∈ Rᵢ with `decodeᵢ(p′) = decodeⱼ(p)` and `ε = [0,0]`. When defined, `↓ⱼᵢ(j, p, ε, d, π) = (i, p′, [0,0], d, π)`.

### 4.8 Policy Function

**Definition 7 (Policy).** A policy is a triple **π_pol = (τ, β, δ)**: τ ∈ ℝ⁺ a maximum tolerated error-interval width, β ∈ ℝ⁺ ∪ {∞} a maximum cost budget, δ ∈ {true, false} a determinism requirement.

**Definition 8 (Admissibility).** A state σ = (i, p, [lo,hi], d, π), produced at accumulated cost c, is **admissible** under π_pol = (τ, β, δ) iff `(hi − lo) ≤ τ`, `c ≤ β`, and `¬δ ∨ detᵢ = true`.

### 4.9 Cost Function and the Selection Problem

**Definition 9 (Feasible Set).** Given op, input states σ₁,…,σₖ, and π_pol, `F(op, σ₁..σₖ, π_pol) = { j ∈ 𝒯 : apply(op, ↑to-j(σ₁),…,↑to-j(σₖ)) is valid and admissible under π_pol }`.

**Definition 10 (The Selection Problem).** APC's task is `tier*(op, σ₁..σₖ, π_pol) = argmin_{j ∈ F(op,σ₁..σₖ,π_pol)} cost_j(op)` — the minimum-cost tier among those yielding a valid, admissible result.

**A4 (Error Achievability Monotonicity).** If op at tier j yields an admissible result, op at any j′ > j also satisfies the tolerance and determinism components of admissibility.

### 4.10 Dimensional Soundness

**Definition 11 (Dimension Vector).** A dimension vector d ∈ ℤᵏ for k base dimensions (or an application-defined unit family). States combine under addition or comparison only if their dimension vectors are equal; multiplication/division combine vectors additively/subtractively. This is a typing judgment external to the tier chain, constraining which transitions in Definition 4 are defined at all, independent of which tier ultimately performs them.

### 4.11 Termination Condition and Theorems

**Termination Condition.** Under A3, Tₙ satisfies the tolerance and determinism components of admissibility unconditionally for any finite well-defined expression; only the budget component β can fail even at Tₙ, in which case the search halts in a distinct, diagnosable "budget exceeded" outcome rather than proceeding past the top of a finite chain. This is the condition under which Theorem 3 below is proved.

We now state six results. Each proof is a sketch — it identifies the argument and its assumptions, but is not a mechanized derivation; we regard mechanization as necessary future work (§9), not as discharged by what follows.

---

**Theorem 1 (Promotion Preservation).** For i < j and valid σ at tier i, `↑ᵢⱼ(σ)` is valid and value-preserving.

*Proof sketch.* By Definition 5, `decodeⱼ(eᵢⱼ(p)) = decodeᵢ(p)`, and ε is unchanged by promotion, so the containment condition of Definition 3 is preserved verbatim. ∎ (sketch)

---

**Theorem 2 (Demotion Correctness).** `↓ⱼᵢ(σ)` is defined, and its result is valid, if and only if the true value of σ is exactly representable at tier i.

*Proof sketch.* "Only if": `↓ⱼᵢ` is defined exactly when Definition 6's existential condition holds, so definedness entails representability by construction. "If": given representable p′ with `decodeᵢ(p′) = decodeⱼ(p)`, the resulting state (i, p′, [0,0], d, π) is valid because its true value equals `decodeᵢ(p′)` exactly, per the hypothesis `ε = [0,0]` on the pre-demotion state, so Definition 3's containment holds trivially. ∎ (sketch) This is the formal counterpart of the informal principle "demotion is only safe when provably lossless": Definition 6 makes "provably lossless" precise, and this theorem shows it is exactly necessary and sufficient, not merely sufficient.

---

**Theorem 3 (Cascade Termination).** For every op, inputs, and π_pol with β not violated at Tₙ, the search for tier*(op,σ₁..σₖ,π_pol) terminates in at most n − i₀ steps, returning either an admissible result or a diagnosable budget-exceeded condition.

*Proof sketch.* 𝒯 is finite, so a search considering each tier at most once terminates in bounded steps. By A3, Tₙ satisfies tolerance unconditionally (zero-width error) and can be constructed to satisfy determinism unconditionally (a canonical, platform-independent exact evaluation order), so only β can fail at Tₙ; when it does, there is no higher tier to try, and the search halts in the budget-exceeded outcome rather than continuing indefinitely. In both cases, halting is guaranteed. ∎ (sketch)

---

**Theorem 4 (Error Monotonicity / No Silent Narrowing).** For any transition (Definition 4) deriving σ′ from σ without a tier change or recomputation from a strictly higher tier's exact payload, the propagation rule computing σ′'s interval from σ's must be *inflationary*: it may widen the interval to account for its own rounding contribution, but must never report an interval narrower than the operands' intervals justify.

*Proof sketch.* This is a well-formedness requirement imposed on every primitive operation's propagation rule (an example is given in §5.3), not a consequence of Definitions 1–11 alone; we state it as a proof obligation made explicit rather than left implicit. Given that every primitive rule is constructed to be inflationary, validity (Definition 3) is preserved by structural induction on the expression tree: the base case (a literal, or a value freshly promoted per Theorem 1) is valid by construction, and each inductive step preserves validity because the rule never removes justified uncertainty. ∎ (sketch)

---

**Theorem 5 (Tier Monotonicity of Capability).** For i < j, every value exactly representable at Tᵢ is exactly representable at Tⱼ, and promotion to any k > j does not un-represent it.

*Proof sketch.* Immediate from A1 (Rᵢ ⊆ Rⱼ for i < j) and transitivity of ⊆. ∎ (sketch) Stated separately from Theorem 1 because it is a static property of the chain (R₀ ⊆ R₁ ⊆ … ⊆ Rₙ), while Theorem 1 is a property of the promotion operation acting on a specific state; both are needed for Theorem 6.

---

**Theorem 6 (Optimality of Greedy Escalation).** Under A1–A4, the following procedure computes tier*(op,σ₁..σₖ,π_pol) exactly: start at the minimal tier i₀ containing all inputs (well-defined by Theorem 5); evaluate op at successively higher tiers; stop at the first tier j yielding a valid, admissible result; if no tier through Tₙ succeeds, report budget-exceeded.

*Proof sketch.* By A4, the tolerance/determinism components of admissibility are upward-closed in tier index, so the feasible set restricted to them is a tail {j, j+1, …, n} of 𝒯 for some minimal j. By A2, cost is non-decreasing in tier, so the cheapest tier in that tail is its minimal element j — exactly the first tier the greedy ascent accepts on tolerance/determinism grounds. The budget constraint does not change this conclusion: since cost is non-decreasing, if budget is violated at j it may be violated at every tier examined so far, and no cheaper tier below j is feasible on tolerance/determinism grounds (by minimality of j); the greedy procedure's correct response — keep ascending, or report budget-exceeded at Tₙ — is exactly Theorem 3's termination behavior. Hence the greedy procedure's first accepted tier equals `argmin_{j∈F} cost_j(op)`. ∎ (sketch)

*Discussion.* We regard Theorem 6 as the paper's central formal claim: it shows the informally described "keep trying higher tiers" procedure is not a heuristic but the exact solution to Definition 10's optimization problem, given A2 and A4. §7 identifies workloads under which A4 specifically can be threatened by a careless implementation, and what discipline is required to avoid that.

---

## 5. Reference Implementation Strategy

§4 defines APC and its environment as a model; it does not mandate an implementation. This section presents one strategy for realizing that model on conventional compiled, multi-threaded hardware, stating at each step which formal property it targets, and reiterating that alternative realizations (interpreted, GPU-resident, distributed, mechanically verified) are equally valid instantiations of §4, provided they satisfy Definitions 1–11 and, ideally, A1–A4.

### 5.1 Instantiating the tier chain

| Tier | Representation | Rᵢ (informally) | Physical form |
|---|---|---|---|
| T0 | Native small integer | small integers | inline in register |
| T1 | Native wide integer | larger fixed-range integers | inline in register |
| T2 | Native binary float | binary-representable reals in range | inline (NaN-boxed) |
| T3 | Double-double / compensated float pair | as T2, reduced rounding error | inline pair |
| T4 | Arbitrary-precision integer | all integers | heap pointer |
| T5 | Arbitrary-precision rational | all rationals | heap pointer |
| T6 | Arbitrary-precision decimal | all finite decimals | heap pointer |
| T7 | Symbolic/deferred expression | all of ℝ, deferred | arena or heap pointer |

A1 and A3 are satisfied by construction of this list. A2 (cost monotonicity) is a design target of §5.2–5.6, not an automatic consequence of the table; a poor implementation of a high tier could violate it, which would threaten Theorem 6's applicability to that implementation, and we flag this dependency rather than asserting it away.

### 5.2 Compressed value representation

Values are held, where possible, in a single 64-bit machine word using tagged-pointer/NaN-boxing techniques (§3): T0–T2 inline, tier tag in unused bits; T4+ switch the word to a heap pointer carrying the full Definition 2 tuple. This targets low per-value memory and cache overhead; we treat this as a design target pending the cache-behavior measurements of §8.

### 5.3 Compiled fast path and hardware-trapped escalation

Where monomorphization (§5.6) fixes a static tier, arithmetic compiles to native instructions relying on hardware overflow/exception flags to detect the boundary conditions requiring escalation:

```
compiled_add(a, b):
    result = a + b
    trap_if_overflow -> cold_escalate(a, b)     // predicted not-taken
```

The intent is that Theorem 6's search machinery is paid only inside `cold_escalate`, on the branch proven insufficient. An illustrative propagation rule discharging Theorem 4's proof obligation, for addition at an interval-tracking tier:

```
add(σ_a, σ_b):
    value  = decode(σ_a.p) + decode(σ_b.p)
    lo     = σ_a.lo + σ_b.lo − rounding_ulp(value)
    hi     = σ_a.hi + σ_b.hi + rounding_ulp(value)
    return (tier, encode(value), [lo, hi], combine_dims(σ_a.d, σ_b.d), ⊥)
```

This rule is inflationary as Theorem 4 requires: it only adds a non-negative rounding term.

### 5.4 Bounded lazy deferral

T7 provenance is bounded: expressions composed entirely of compile-time constants are folded during compilation rather than deferred; runtime-dependent trees are allocated in a thread-local bump arena with a hard byte budget, forcing materialization once reached — operationalizing the β constraint of Definition 7 for the T7 case.

### 5.5 Dimensional checking

Dimension vectors (Definition 11) are resolved statically wherever possible, so the typing judgment costs nothing at run time in the common case; only genuinely dynamic dimensional combinations incur a runtime check.

### 5.6 Monomorphization as the primary realization of Theorem 6's search

Rather than an interpreted loop per operation, the search of Theorem 6 is resolved, wherever static information permits, as a **compile-time** search: for each (operation, policy) combination in a program, the compiler determines, from static bounds where available, the tier greedy ascent would select, and emits a specialized code path with sufficiency checks elided where statically known to pass. Where the tier cannot be determined statically, the search is realized as an explicit runtime procedure entered only from the cold path of §5.3.

*A caution relative to A4:* within-tier algorithm selection (e.g., a verified-division algorithm chosen over an unverified one, §5.6.1) must remain a strictly cost-side decision and never a validity-side one — it must never change whether a tier *can* meet a tolerance, only how expensively it does so — or A4, and hence Theorem 6, would no longer apply straightforwardly to that implementation.

**5.6.1 Within-tier algorithm selection.** Division/modulo prefer a verified algorithm (accepting a quotient only after confirming `quotient × divisor + remainder = dividend` at the same tier) over an unverified one; verification failure is treated as the "definite failure" that drives escalation in Theorem 6's search. Repeated accumulation prefers compensated summation past a term-count threshold; subtraction of near-equal large magnitudes is a preemptive escalation trigger.

### 5.7 Concurrency and memory lifecycle

Heap-backed states (T4+) shared across threads are classified in two ways. **Thread-local** states, proven confined by escape analysis, live in a thread-local bump arena with no synchronization, reclaimed in bulk at scope end. **Cross-thread** states default to **epoch-based reclamation** (appropriate because escalated payloads are immutable once computed): each thread pins a global epoch with an uncontended store before access and clears it after; retirement is deferred until no thread could hold a pin from before it. Long-lived, unpredictably shared values instead use **biased thread-local reference counting**, reconciled into a shared atomic count only at ownership-transfer points. Reclamation bookkeeping is kept physically separate from payloads to avoid false sharing, and arenas/reclamation lists are allocated NUMA-locally by default. This subsection is a design target evaluated against Definition 3's validity requirement (a reclaimed-too-early value trivially violates it), not a claim proven within §4's model, which does not yet give a formal concurrent semantics (§9).

---

## 6. What Is Actually Novel: A Direct Statement

- **Not novel:** any individual numeric representation (bignums, rationals, decimals, intervals), any individual systems technique (tagged pointers, escape analysis, epoch reclamation, monomorphization), or the general idea that software should sometimes use higher-precision arithmetic when needed.
- **Not the paper's contribution, though necessary scaffolding:** the Universal Numeric Substrate as a tier chain. It is a disciplined arrangement of existing representations, required for APC to have something to plan over, but it introduces no new representation of numbers.
- **Novel, to our knowledge:** the Adaptive Precision Cascade — the formulation of representation selection as the single constrained optimization problem of Definition 10, spanning compile time and run time as one search space; the identification of the exact monotonicity conditions (A2, A4) under which a simple greedy procedure solves it exactly (Theorem 6); and the analysis of §2 showing that four existing postures toward numeric correctness are each a degenerate special case of this one general problem rather than independent alternatives to it.

We consider Theorem 6, together with the classification argument of §2, the paper's principal claims, and everything else — the tier chain, the reference implementation, the concurrency strategy — supporting material establishing that the problem APC solves is well-posed (§4) and realizable without reintroducing the costs it eliminates (§5).

---

## 7. Failure Modes

An adaptive planner's weaknesses are best catalogued by its author before its reviewers. For each failure mode we state what it is, how it would be detected, how it is mitigated today, and what guarantee (if any) remains in force while it occurs.

**Pathological symbolic growth.** *What:* T7 provenance trees can grow faster than the arena budget is checked if many sub-terms are constructed in a tight loop before a materialization trigger fires. *Detection:* arena byte-count crossing the budget threshold. *Mitigation:* forced materialization at budget (§5.4); no mitigation yet exists for the *frequency* of forced materializations under adversarial term-construction rates, only their total memory bound. *Remaining guarantee:* Definition 3 validity is preserved regardless (materialization always occurs at a tier satisfying admissibility), but performance can degrade sharply — this is a performance failure mode, not a correctness one.

**Excessive tier oscillation.** *What:* a workload whose values repeatedly cross a tier boundary defeats the branch-predictor assumption underlying §5.3's fast path. *Detection:* a high ratio of cold-path entries to total operations, measurable at run time. *Mitigation:* none beyond falling back to the runtime search of §5.6, which remains correct but loses the near-zero marginal cost of the predicted-not-taken fast path. *Remaining guarantee:* Theorem 6's optimality still holds per-operation; only the constant-factor cost target of §5.3 is lost, not correctness.

**Policy misconfiguration.** *What:* a policy (τ, β, δ) that is jointly unsatisfiable even at Tₙ (for instance, β set below Tₙ's minimum cost for a given operation) or that is set inconsistently across a scope boundary (a caller demanding determinism from a callee compiled under a non-deterministic policy). *Detection:* the budget-exceeded outcome of Theorem 3 for the unsatisfiable case; a static or dynamic policy-compatibility check at scope boundaries for the inconsistency case. *Mitigation:* the former is handled by design (Theorem 3 defines this as an explicit diagnosable outcome, not a crash); the latter requires a policy-compatibility check we have specified informally (§4.8) but not yet formalized as part of the transition relation of Definition 4 — we regard this as an open modeling gap, not a solved problem. *Remaining guarantee:* the model never silently substitutes a policy other than the one requested; it fails closed (diagnosable) rather than open (silent).

**Metadata exhaustion.** *What:* the per-state overhead of carrying an error interval, dimension vector, and provenance handle (Definition 2) at T4+ is bounded per value but not bounded in aggregate for a program holding very many simultaneously live escalated values. *Detection:* heap growth attributable to Envelope metadata rather than payload data. *Mitigation:* none specific beyond ordinary garbage collection or arena/epoch reclamation (§5.7) reclaiming values once they are no longer live; we do not currently have a mechanism to compress metadata across many co-live escalated values (e.g., sharing a dimension vector across a large homogeneous collection), and consider this open. *Remaining guarantee:* correctness (Definition 3) is unaffected; this is purely a memory-footprint concern.

**Pessimistic interval propagation.** *What:* plain (non-affine) interval arithmetic is known in the literature to overestimate error when a computation reuses a variable correlated with itself (e.g., an expression algebraically equal to a constant can still report a nonzero interval). *Detection:* an interval width that does not shrink even after operations that should be exact by algebraic identity. *Mitigation:* affine arithmetic (§3) mitigates but does not eliminate this for all expression shapes; UNS's T3 tier is a partial answer for the specific case of compensated summation, not a general one. *Remaining guarantee:* Theorem 4 (no silent narrowing) still holds — the failure mode is over-conservatism, which can trigger unnecessary escalation, but never produces an invalid state.

**Worst-case allocation pressure / NUMA migration.** *What:* under §5.7's scheme, a value produced on one NUMA node and then accessed in a tight loop by threads on several other nodes incurs repeated cross-node epoch-pin traffic. *Detection:* cross-node memory traffic counters attributable to shared escalated values. *Mitigation:* the NUMA-local default optimizes for the common producer-local-consumer case and does not resolve this adversarial access pattern; no further mitigation is proposed here. *Remaining guarantee:* correctness and race-freedom are unaffected; this is purely a throughput concern under an access pattern we consider adversarial rather than typical.

We do not believe a paper of this scope should claim to resolve all of the above; we list them, with their current mitigation status stated honestly, so that the record is complete rather than left for a reviewer or future implementer to discover independently.

---

## 8. Evaluation Framework

Layer 2's design targets are not, at this stage, supported by measurement, and this paper does not present fabricated or estimated performance numbers in their place. We instead specify the evaluation framework a prototype should be measured against, across four categories, following the view that a systems contribution of this kind should be judged on more than raw throughput.

**Correctness.** Number of overflow events correctly escalated rather than silently wrapped, across a corpus of programs with known range requirements; number of precision-loss events correctly escalated rather than silently accepted, measured against a ground-truth arbitrary-precision oracle; rate of bit-for-bit numerical reproducibility across platforms under a determinism-required policy, measured against the same program compiled with a non-adaptive baseline.

**Performance.** Runtime overhead of the compiled fast path (§5.3) relative to hand-written checked native arithmetic, on workloads that never escalate; cache-miss and allocation-rate effects attributable to the compressed Envelope representation (§5.2) and to T4+ heap allocation; branch-misprediction rate attributable to the cold-path trap under both typical and the oscillating-tier adversarial pattern of §7.

**Adaptivity.** Promotion and demotion frequency across representative workload classes (scientific, financial/decimal-heavy, general application code); frequency of policy-driven escalation versus operand-driven escalation; frequency and cost distribution of tier oscillation events (§7); frequency with which escape analysis (§5.7) succeeds versus falls back conservatively to cross-thread reclamation.

**Developer Experience.** Reduction, relative to a fixed-width-typed baseline, in the number of explicit numeric type annotations, explicit overflow checks, and explicit precision-handling code paths a developer must write to achieve an equivalent correctness guarantee; number of the failure classes of §1.1 that a representative defect corpus shows are prevented by construction rather than by developer vigilance; qualitative assessment of whether code complexity, measured by an existing complexity metric, decreases for numerically sensitive modules under APC relative to the baseline.

We regard the fourth category as easy to omit and important not to: if APC's chief benefit is reduced programmer burden rather than raw speed, an evaluation that measures only performance would understate the contribution's actual value, and we commit to reporting it alongside the first three in any future empirical work building on this paper.

---

## 9. Scope and Relationship to Future Work

This paper is scoped to a formal model (§4) with proof sketches, a philosophical argument for why the problem should be treated as dynamic rather than static (§2), one reference implementation strategy (§5) with explicitly labeled design targets, a catalogue of failure modes with honest mitigation status (§7), and an evaluation framework not yet executed (§8). We consider the following separate, necessary pieces of work that this paper is a prerequisite for:

- A **mechanized proof** of Theorems 1–6 in a proof assistant, in particular formalizing Theorem 4's proof obligation as a property of concrete propagation rules rather than an assumption, and formalizing the policy-compatibility check flagged as an open gap in §7's discussion of policy misconfiguration.
- A **working prototype compiler** implementing the strategy of §5, sufficient to execute the evaluation framework of §8.
- A **concurrent extension of the formal model**, since §4 does not currently give a formal semantics to the multi-threaded reclamation strategy of §5.7, justified there only informally against Definition 3.
- A **generalization from a tier chain to a tier lattice** (§4.2), admitting incomparable specialized representations, which the present total-order treatment does not cover.
- A **metadata-compression scheme** for aggregates of many co-live escalated values, addressing the metadata-exhaustion failure mode of §7, which this paper identifies but does not solve.

---

## 10. Conclusion

Numeric software fails, recurringly, because the choice of representation is fixed once, early, and permanently, by a person who cannot yet know everything the system will later know about its data. We have argued this choice should instead be treated as a constrained optimization problem, defended that position against four natural alternatives by showing each to be a degenerate special case of the general problem rather than an independent solution to it (§2), stated the problem formally as the Adaptive Precision Cascade operating over the Universal Numeric Substrate (§4), and shown, under two explicit monotonicity assumptions, that a simple greedy search solves it exactly (Theorem 6) — the contribution we consider genuinely new in this work, as distinct from the tier chain and systems techniques it composes (§6). We have described one implementation strategy realizing this without the costs a naive approach would incur (§5), catalogued where that strategy is known to strain (§7), and specified, rather than fabricated, the evidence a future prototype would need to produce (§8). This paper is offered as the foundation for that subsequent work, not as a substitute for it.

---

## References (representative prior art)

1. IEEE Standard for Floating-Point Arithmetic (IEEE 754).
2. W. Kahan, "Pracniques: Further Remarks on Reducing Truncation Errors" (compensated summation).
3. R. E. Moore, "Interval Analysis."
4. J. Stolfi and L. H. de Figueiredo, "Self-Validated Numerical Methods and Applications" (affine arithmetic, and the interval dependency problem).
5. J. L. Gustafson, "The End of Error: Unum Computing" (posit/unum arithmetic).
6. GNU Multiple Precision Arithmetic Library (GMP); GNU MPFR Library (arbitrary-precision floating point with correct rounding).
7. Prior art in dimensional/units-of-measure type systems in general-purpose and scientific languages.
8. N-version programming and redundant computation in safety-critical software engineering literature.
9. NaN-boxing and tagged-pointer value representations in high-performance dynamic language runtime implementation literature.
10. K. Fraser, "Practical Lock-Freedom" (epoch-based reclamation); read-copy-update (RCU) literature from operating-systems kernel engineering.
11. Biased and thread-local reference counting strategies in functional-language runtime implementation literature.
12. Monomorphization strategies in statically compiled generic-programming systems.
13. P. Cousot and R. Cousot, foundational work on abstract interpretation, as background for relating floating-point program behavior to idealized real-number semantics.
14. Static analysis and verification tool literature for floating-point-heavy safety-critical software (interval/affine-arithmetic-based analyzers).
15. Query optimization and cost-based execution planning literature in database systems, as a structural analogue to APC's role relative to the representations it plans over.
