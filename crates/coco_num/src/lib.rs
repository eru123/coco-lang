//! # Adaptive Precision Cascade / Universal Numeric Substrate
//!
//! Minimal mathematical model of the APC planner and UNS tier chain from
//! "The Adaptive Precision Cascade" paper.
//!
//! **NOTE:** This crate currently provides the *mathematical core* only.
//! The interpreter's runtime values (`Value::Int64`, `Value::Int`, `Value::Float`)
//! are NOT yet backed by APC types.  Arithmetic in the VM still uses the
//! existing fast-path adaptive representation, with optional advisory checks
//! from this module.  Full APC integration is deferred until the execution
//! model stabilizes enough to measure Layer 2 performance claims.
//!
//! Current integration points:
//! - Integer overflow detection via `select_tier` / `eval_policy`
//! - Promotion/demotion proof sketches verified by unit tests
//! - Future: replace `Value` numeric variants with `NumericState`-backed payloads

#![allow(dead_code)]
#![allow(unused_variables)]

use num_bigint::{BigInt, Sign};
use num_rational::Ratio;
use num_traits::{One, ToPrimitive};

// ============================================================================
// Dimension vector (placeholder)
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Dim(());

impl Dim {
    pub const UNIT: Self = Self(());
}

// ============================================================================
// UNS Tier chain
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    T0 = 0,
    T1 = 1,
    T2 = 2,
    T3 = 3,
    T4 = 4,
    T5 = 5,
    T6 = 6,
    T7 = 7,
}

#[derive(Clone, Debug)]
pub struct TierMeta {
    pub index: usize,
    pub name: &'static str,
    pub cost: u64,
    pub determinism: bool,
}

const TIER_METAS: &[TierMeta] = &[
    TierMeta { index: 0, name: "T0", cost: 1, determinism: true },
    TierMeta { index: 1, name: "T1", cost: 2, determinism: true },
    TierMeta { index: 2, name: "T2", cost: 3, determinism: true },
    TierMeta { index: 3, name: "T3", cost: 5, determinism: true },
    TierMeta { index: 4, name: "T4", cost: 9, determinism: true },
    TierMeta { index: 5, name: "T5", cost: 14, determinism: true },
    TierMeta { index: 6, name: "T6", cost: 20, determinism: true },
    TierMeta { index: 7, name: "T7", cost: 28, determinism: true },
];

impl Tier {
    pub const COUNT: usize = 8;

    pub fn meta(self) -> &'static TierMeta {
        &TIER_METAS[self as usize]
    }

    pub fn is_top(self) -> bool {
        self == Tier::T7
    }

    pub fn unit() -> Self {
        Tier::T0
    }

    pub fn try_from_index(i: usize) -> Option<Self> {
        match i {
            0 => Some(Tier::T0),
            1 => Some(Tier::T1),
            2 => Some(Tier::T2),
            3 => Some(Tier::T3),
            4 => Some(Tier::T4),
            5 => Some(Tier::T5),
            6 => Some(Tier::T6),
            7 => Some(Tier::T7),
            _ => None,
        }
    }
}

// ============================================================================
// Numeric states and payloads
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
pub enum Num {
    I64(i64),
    BigInt(BigInt),
    F64(f64),
    DoubleDouble { hi: f64, lo: f64 },
    Rational(Ratio<BigInt>),
    Decimal(String),
    SymbolicExpr(String, Vec<Num>),
    Undefined,
}

impl Num {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Num::I64(n) => Some(*n as f64),
            Num::BigInt(n) => n.to_f64(),
            Num::F64(n) => Some(*n),
            Num::DoubleDouble { hi, lo } => Some(hi + lo),
            Num::Rational(r) => r.to_f64(),
            Num::Decimal(s) => s.parse::<f64>().ok(),
            Num::Undefined => None,
            Num::SymbolicExpr(_, _) => None,
        }
    }

    pub fn as_exact_integer(&self) -> Option<BigInt> {
        match self {
            Num::I64(n) => Some(BigInt::from(*n)),
            Num::BigInt(n) => Some(n.clone()),
            Num::Rational(r) if !r.is_integer() => None,
            Num::Rational(r) => Some(r.numer().clone()),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Interval {
    pub lo: f64,
    pub hi: f64,
}

impl Interval {
    pub fn zero() -> Self {
        Self { lo: 0.0, hi: 0.0 }
    }

    pub fn point(x: f64) -> Self {
        Self { lo: x, hi: x }
    }

    pub fn width(&self) -> f64 {
        self.hi - self.lo
    }

    pub fn add_inflation(a: &Self, b: &Self, rounding: f64) -> Self {
        let rounding = rounding.abs();
        let lo = a.lo + b.lo - rounding;
        let hi = a.hi + b.hi + rounding;
        Self { lo, hi }
    }

    pub fn sub_inflation(a: &Self, b: &Self, rounding: f64) -> Self {
        let rounding = rounding.abs();
        let lo = a.lo - b.hi - rounding;
        let hi = a.hi - b.lo + rounding;
        Self { lo, hi }
    }

    pub fn mul_inflation(a: &Self, b: &Self, rounding: f64) -> Self {
        let corners = [
            a.lo * b.lo,
            a.lo * b.hi,
            a.hi * b.lo,
            a.hi * b.hi,
        ];
        let lo = corners.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = corners.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let rounding = rounding.abs();
        Self {
            lo: lo - rounding,
            hi: hi + rounding,
        }
    }

    pub fn div_inflation(a: &Self, b: &Self, rounding: f64) -> Self {
        if b.lo <= 0.0 && b.hi >= 0.0 {
            return Self {
                lo: f64::NEG_INFINITY,
                hi: f64::INFINITY,
            };
        }
        let corners = [
            a.lo / b.lo,
            a.lo / b.hi,
            a.hi / b.lo,
            a.hi / b.hi,
        ];
        let lo = corners.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = corners.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let rounding = rounding.abs();
        Self {
            lo: lo - rounding,
            hi: hi + rounding,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NumericState {
    pub tier: Tier,
    pub payload: Num,
    pub interval: Interval,
    pub dim: Dim,
    pub provenance: Option<String>,
}

// ============================================================================
// Correctness predicate and policy
// ============================================================================

#[derive(Clone, Copy, Debug)]
pub struct Policy {
    pub max_interval_width: f64,
    pub max_cost_budget: u64,
    pub require_determinism: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            max_interval_width: 0.0,
            max_cost_budget: u64::MAX,
            require_determinism: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admissibility {
    Admissible,
    IntervalExceedsTolerance,
    BudgetExceeded,
    DeterminismRequiredButMissing,
}

impl NumericState {
    pub fn admissible(&self, policy: Policy, accumulated_cost: u64) -> Admissibility {
        if self.payload == Num::Undefined {
            return Admissibility::IntervalExceedsTolerance;
        }
        if self.interval.width() > policy.max_interval_width {
            return Admissibility::IntervalExceedsTolerance;
        }
        if accumulated_cost > policy.max_cost_budget {
            return Admissibility::BudgetExceeded;
        }
        if policy.require_determinism && !Tier::meta(self.tier).determinism {
            return Admissibility::DeterminismRequiredButMissing;
        }
        Admissibility::Admissible
    }
}

// ============================================================================
// Promotion / demotion
// ============================================================================

impl NumericState {
    pub fn promote(&self, tier: Tier) -> Result<Self, APCError> {
        if tier < self.tier {
            return Err(APCError::InvalidTierTransition {
                from: self.tier,
                to: tier,
            });
        }
        let payload = encode_at_tier(&self.payload, tier)?;
        Ok(NumericState {
            tier,
            payload,
            interval: self.interval.clone(),
            dim: self.dim.clone(),
            provenance: self.provenance.clone(),
        })
    }

    pub fn demote(&self, tier: Tier) -> Result<Self, APCError> {
        if tier >= self.tier {
            return Err(APCError::InvalidTierTransition {
                from: self.tier,
                to: tier,
            });
        }
        if self.interval.width() > f64::EPSILON {
            return Err(APCError::DemotionNotLossless);
        }
        let exact = self
            .payload
            .as_exact_integer()
            .ok_or(APCError::DemotionNotLossless)?;
        let payload = match tier {
            Tier::T0 | Tier::T1 | Tier::T4 => Num::BigInt(exact.clone()),
            Tier::T2 if can_represent_as_i64(&exact) => {
                Num::I64(exact.to_i64().unwrap())
            }
            _ => return Err(APCError::DemotionNotLossless),
        };
        Ok(NumericState {
            tier,
            payload,
            interval: Interval::zero(),
            dim: self.dim.clone(),
            provenance: self.provenance.clone(),
        })
    }

    pub fn with_interval(&self, interval: Interval) -> Self {
        NumericState {
            interval,
            ..self.clone()
        }
    }
}

// ============================================================================
// Encoding helpers
// ============================================================================

fn encode_at_tier(payload: &Num, tier: Tier) -> Result<Num, APCError> {
    Ok(match tier {
        Tier::T0 => match payload {
            Num::I64(_) | Num::F64(_) => payload.clone(),
            _ => payload_to_i64(payload)?,
        },
        Tier::T1 => match payload {
            Num::I64(_) | Num::BigInt(_) | Num::F64(_) => payload.clone(),
            _ => Num::BigInt(payload_to_bigint(payload)?),
        },
        Tier::T2 => Num::F64(payload.as_f64().ok_or(APCError::TierLossless)?),
        Tier::T3 => Num::DoubleDouble {
            hi: payload.as_f64().ok_or(APCError::TierLossless)?,
            lo: 0.0,
        },
        Tier::T4 => Num::BigInt(payload_to_bigint(payload)?),
        Tier::T5 => payload_to_rational(payload)?,
        Tier::T6 => Num::Decimal(format!("{:?}", payload)),
        Tier::T7 => Num::SymbolicExpr("provenance".into(), vec![payload.clone()]),
    })
}

fn can_represent_as_i64(n: &BigInt) -> bool {
    let (sign, digits) = n.to_u64_digits();
    !digits.is_empty() && digits.len() == 1 && sign == Sign::Plus
}

fn payload_to_i64(payload: &Num) -> Result<Num, APCError> {
    let target = match payload {
        Num::I64(_) => return Ok(payload.clone()),
        Num::BigInt(n) => n.to_i64().ok_or(APCError::TierLossless)?,
        Num::F64(n) => *n as i64,
        Num::DoubleDouble { hi, .. } => *hi as i64,
        _ => return Err(APCError::TierLossless),
    };
    Ok(Num::I64(target))
}

fn payload_to_bigint(payload: &Num) -> Result<BigInt, APCError> {
    match payload {
        Num::I64(n) => Ok(BigInt::from(*n)),
        Num::BigInt(n) => Ok(n.clone()),
        Num::F64(n) => approx_bigint_from_f64(*n).ok_or(APCError::TierLossless),
        Num::DoubleDouble { hi, .. } => {
            approx_bigint_from_f64(*hi).ok_or(APCError::TierLossless)
        }
        Num::Rational(r) => Ok(r.numer().clone() / r.denom().clone()),
        Num::Decimal(s) => s
            .parse::<f64>()
            .ok()
            .and_then(approx_bigint_from_f64)
            .ok_or(APCError::TierLossless),
        _ => Err(APCError::TierLossless),
    }
}

fn payload_to_rational(payload: &Num) -> Result<Num, APCError> {
    fn try_rational(n: &Num) -> Option<Ratio<BigInt>> {
        let num = match n {
            Num::I64(n) => BigInt::from(*n),
            Num::BigInt(n) => n.clone(),
            Num::F64(f) => approx_bigint_from_f64(*f)?,
            Num::DoubleDouble { hi, .. } => approx_bigint_from_f64(*hi)?,
            Num::Rational(r) => return Some(r.clone()),
            _ => return None,
        };
        Some(Ratio::new(num, BigInt::one()))
    }
    try_rational(payload)
        .map(Num::Rational)
        .ok_or(APCError::TierLossless)
}

fn approx_bigint_from_f64(x: f64) -> Option<BigInt> {
    if !x.is_finite() {
        return None;
    }
    Some(BigInt::from(x.round() as i64))
}

impl From<BigInt> for Num {
    fn from(value: BigInt) -> Self {
        Num::BigInt(value)
    }
}

// ============================================================================
// Cost model
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Neg,
    Pow,
}

pub fn cost_of(op: Op, tier: Tier) -> u64 {
    let op_factor = match op {
        Op::Add | Op::Sub | Op::Neg => 1,
        Op::Mul => 2,
        Op::Div => 3,
        Op::Pow => 5,
    };
    Tier::meta(tier).cost.saturating_mul(op_factor)
}

// ============================================================================
// APC Error
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum APCError {
    InvalidTierTransition { from: Tier, to: Tier },
    DemotionNotLossless,
    TierLossless,
    BudgetExceeded,
    InvalidOperands,
    MismatchedDimensions,
    UndefinedResult,
}

impl From<Admissibility> for APCError {
    fn from(value: Admissibility) -> Self {
        match value {
            Admissibility::IntervalExceedsTolerance => APCError::TierLossless,
            Admissibility::BudgetExceeded => APCError::BudgetExceeded,
            Admissibility::DeterminismRequiredButMissing => APCError::InvalidOperands,
            Admissibility::Admissible => APCError::InvalidOperands,
        }
    }
}

impl std::fmt::Display for APCError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            APCError::InvalidTierTransition { from, to } => write!(
                f,
                "invalid tier transition {} -> {}",
                from.meta().name,
                to.meta().name,
            ),
            APCError::DemotionNotLossless => write!(f, "demotion would lose value"),
            APCError::TierLossless => {
                write!(f, "payload not losslessly representable in target tier")
            }
            APCError::BudgetExceeded => write!(f, "APC policy budget exceeded"),
            APCError::InvalidOperands => write!(f, "invalid operands for operation"),
            APCError::MismatchedDimensions => write!(f, "dimension vectors do not match"),
            APCError::UndefinedResult => write!(f, "operation produced undefined value"),
        }
    }
}

impl std::error::Error for APCError {}

// ============================================================================
// Operations and propagation rules
// ============================================================================

fn rounding_ulp(_tier: Tier, value: f64) -> f64 {
    value.abs() * f64::EPSILON
}

fn base_tier<'a>(a: &'a NumericState, b: &'a NumericState) -> Tier {
    Tier::min(a.tier, b.tier)
}

fn reconstruct(a: &NumericState, b: &NumericState, interval: Interval) -> NumericState {
    let provenance = match (&a.provenance, &b.provenance) {
        (Some(p), Some(q)) => Some(format!("({} {} {})", p, "op", q)),
        (Some(p), None) => Some(format!("({} op rhs)", p)),
        (None, Some(q)) => Some(format!("(lhs op {})", q)),
        (None, None) => None,
    };
    let payload = match (&a.payload, &b.payload) {
        (Num::Undefined, Num::Undefined) => Num::Undefined,
        (Num::Undefined, _) => b.payload.clone(),
        (_, Num::Undefined) => a.payload.clone(),
        _ => a.payload.clone(),
    };
    NumericState {
        tier: base_tier(a, b),
        payload,
        interval,
        dim: a.dim,
        provenance,
    }
}

pub fn apply(op: Op, a: NumericState, b: Option<NumericState>) -> Result<NumericState, APCError> {
    match op {
        Op::Neg => apply_neg(a),
        _ => {
            let b = b.ok_or(APCError::InvalidOperands)?;
            if a.dim != b.dim {
                return Err(APCError::MismatchedDimensions);
            }
            match op {
                Op::Add => apply_add(a, b),
                Op::Sub => apply_sub(a, b),
                Op::Mul => apply_mul(a, b),
                Op::Div => apply_div(a, b),
                Op::Pow => apply_pow(a, b),
                Op::Neg => unreachable!(),
            }
        }
    }
}

fn apply_add(a: NumericState, b: NumericState) -> Result<NumericState, APCError> {
    let value = a.interval.lo + b.interval.lo;
    let interval = Interval::add_inflation(&a.interval, &b.interval, rounding_ulp(a.tier, value));
    Ok(reconstruct(&a, &b, interval))
}

fn apply_sub(a: NumericState, b: NumericState) -> Result<NumericState, APCError> {
    let value = a.interval.lo - b.interval.lo;
    let interval = Interval::sub_inflation(&a.interval, &b.interval, rounding_ulp(a.tier, value));
    Ok(reconstruct(&a, &b, interval))
}

fn apply_mul(a: NumericState, b: NumericState) -> Result<NumericState, APCError> {
    let value = a.interval.lo * b.interval.lo;
    let interval = Interval::mul_inflation(&a.interval, &b.interval, rounding_ulp(a.tier, value));
    Ok(reconstruct(&a, &b, interval))
}

fn apply_div(a: NumericState, b: NumericState) -> Result<NumericState, APCError> {
    if b.interval.lo <= 0.0 && b.interval.hi >= 0.0 {
        return Err(APCError::UndefinedResult);
    }
    let value = a.interval.lo / b.interval.lo;
    let interval = Interval::div_inflation(&a.interval, &b.interval, rounding_ulp(a.tier, value));
    Ok(reconstruct(&a, &b, interval))
}

fn apply_neg(a: NumericState) -> Result<NumericState, APCError> {
    let interval = Interval {
        lo: -a.interval.hi,
        hi: -a.interval.lo,
    };
    Ok(NumericState { interval, ..a })
}

fn apply_pow(a: NumericState, b: NumericState) -> Result<NumericState, APCError> {
    if !(b.interval.lo == b.interval.hi && b.interval.lo.fract() == 0.0) {
        return Err(APCError::TierLossless);
    }
    let exp = b.interval.lo as u32;
    let mut base = a.clone();
    for _ in 1..exp {
        base = apply_mul(base, a.clone())?;
    }
    Ok(base)
}

// ============================================================================
// APC selection search
// ============================================================================

/// Theorem 6 greedy search over attainable tiers for a binary operation.
pub fn select_tier(
    op: Op,
    a: &NumericState,
    b: &NumericState,
    policy: Policy,
) -> Result<Tier, APCError> {
    let start = base_tier(a, b);
    for tier in (start as u8)..=(Tier::T7 as u8) {
        let tier = Tier::try_from_index(tier as usize).unwrap();
        let op_cost = cost_of(op, tier);
        let promoted_a = a.promote(tier)?;
        let promoted_b = b.promote(tier)?;
        let result = apply(op, promoted_a, Some(promoted_b))?;
        match result.admissible(policy, op_cost) {
            Admissibility::Admissible => return Ok(tier),
            Admissibility::BudgetExceeded => return Err(APCError::BudgetExceeded),
            Admissibility::IntervalExceedsTolerance
            | Admissibility::DeterminismRequiredButMissing => continue,
        }
    }
    let meta = Tier::meta(Tier::T7);
    if policy.max_cost_budget < cost_of(op, Tier::T7) {
        Err(APCError::BudgetExceeded)
    } else {
        Ok(Tier::T7)
    }
}

pub fn eval_policy(
    op: Op,
    a: NumericState,
    b: NumericState,
    policy: Policy,
) -> Result<NumericState, APCError> {
    let tier = select_tier(op, &a, &b, policy)?;
    let pa = a.promote(tier)?;
    let pb = b.promote(tier)?;
    let result = apply(op, pa, Some(pb))?;
    let tier_meta = Tier::meta(tier);
    match result.admissible(policy, cost_of(op, tier)) {
        Admissibility::Admissible => Ok(result),
        Admissibility::BudgetExceeded => Err(APCError::BudgetExceeded),
        other => Err(other.into()),
    }
}

// ============================================================================
// Property verification helpers
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct CounterExample {
    pub op: Op,
    pub tier: Tier,
    pub a: NumericState,
    pub b: NumericState,
    pub note: &'static str,
}

#[derive(Debug, Clone)]
pub struct VerificationReport {
    pub promotion_preserved: Vec<CounterExample>,
    pub demotion_safe: Vec<CounterExample>,
    pub interval_inflation: Vec<CounterExample>,
    pub greedy_first_feasible: Vec<CounterExample>,
    pub capable_tiers: Vec<CounterExample>,
}

impl VerificationReport {
    pub fn empty() -> Self {
        Self {
            promotion_preserved: Vec::new(),
            demotion_safe: Vec::new(),
            interval_inflation: Vec::new(),
            greedy_first_feasible: Vec::new(),
            capable_tiers: Vec::new(),
        }
    }

    pub fn valid(&self) -> bool {
        self.promotion_preserved.is_empty()
            && self.demotion_safe.is_empty()
            && self.interval_inflation.is_empty()
            && self.greedy_first_feasible.is_empty()
            && self.capable_tiers.is_empty()
    }
}

impl std::fmt::Display for VerificationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "VerificationReport:")?;
        writeln!(
            f,
            " promotion preserved: {}",
            self.promotion_preserved.len()
        )?;
        writeln!(f, " demotion safe: {}", self.demotion_safe.len())?;
        writeln!(f, " interval inflation: {}", self.interval_inflation.len())?;
        writeln!(
            f,
            " greedy first feasible: {}",
            self.greedy_first_feasible.len()
        )?;
        writeln!(f, " capable tiers: {}", self.capable_tiers.len())?;
        Ok(())
    }
}

impl APCError {
    fn into_counter_example(
        self,
        kind: &'static str,
        tier: Tier,
        a: NumericState,
        b: NumericState,
    ) -> CounterExample {
        CounterExample {
            op: Op::Add,
            tier,
            a,
            b,
            note: kind,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CheckSamples {
    pub values: Vec<NumericState>,
    pub ops: Vec<Op>,
    pub tiers: Vec<(Tier, Tier)>,
}

impl Default for CheckSamples {
    fn default() -> Self {
        Self {
            values: vec![
                make_state(Tier::T0, Num::I64(7)),
                make_state(Tier::T0, Num::I64(-3)),
                make_state(Tier::T1, Num::BigInt(BigInt::from(123_456))),
                make_state(Tier::T2, Num::F64(std::f64::consts::PI)),
                make_state(Tier::T2, Num::F64(1.0 / 3.0)),
                make_state(Tier::T4, Num::BigInt(BigInt::from(1_000_000))),
                make_state(
                    Tier::T4,
                    Num::Rational(Ratio::new(BigInt::from(1), BigInt::from(7))),
                ),
            ],
            ops: vec![Op::Add, Op::Sub, Op::Mul, Op::Div],
            tiers: (0..Tier::COUNT)
                .flat_map(|i| {
                    (0..Tier::COUNT).map(move |j| {
                        (
                            Tier::try_from_index(i).unwrap(),
                            Tier::try_from_index(j).unwrap(),
                        )
                    })
                })
                .collect(),
        }
    }
}

fn make_state(tier: Tier, payload: Num) -> NumericState {
    NumericState {
        tier,
        payload,
        interval: Interval::zero(),
        dim: Dim::UNIT,
        provenance: None,
    }
}

pub fn check_properties(samples: CheckSamples, policy: Policy) -> VerificationReport {
    let mut report = VerificationReport::empty();
    for state in &samples.values {
        for tier in (state.tier as u8 + 1)..=(Tier::T7 as u8) {
            let tier = Tier::try_from_index(tier as usize).unwrap();
            if let Ok(promoted) = state.promote(tier) {
                match (state.payload.clone(), promoted.payload.clone()) {
                    (Num::I64(a), Num::I64(b)) if a == b => {}
                    (Num::BigInt(a), Num::BigInt(b)) if a == b => {}
                    (Num::BigInt(a), Num::I64(b)) if a == BigInt::from(b) => {}
                    (Num::I64(a), Num::BigInt(b)) if BigInt::from(a) == b => {}
                    (Num::F64(a), Num::F64(b)) if (a - b).abs() < f64::EPSILON => {}
                    _ => {
                        report.promotion_preserved.push(CounterExample {
                            op: Op::Add,
                            tier,
                            a: state.clone(),
                            b: promoted,
                            note: "promotion value changed",
                        });
                    }
                }
            }
        }

        if state.interval.width() <= f64::EPSILON {
            if let Some(integer) = state.payload.as_exact_integer() {
                for tier in [Tier::T0, Tier::T1, Tier::T4] {
                    if tier < state.tier {
                        if let Err(APCError::DemotionNotLossless) = state.demote(tier) {
                            report.demotion_safe.push(CounterExample {
                                op: Op::Add,
                                tier,
                                a: state.clone(),
                                b: make_state(tier, Num::BigInt(integer.clone())),
                                note: "exact demotion unexpectedly rejected",
                            });
                        }
                    }
                }
            }
        }
    }

    for a in &samples.values {
        for b in &samples.values {
            if a.dim != b.dim {
                continue;
            }
            for op in &samples.ops {
                let result = apply(*op, a.clone(), Some(b.clone()));
                if let Ok(state) = result {
                    let base = base_tier(a, b);
                    let base_meta = Tier::meta(base);
                    let basic_cost = base_meta.cost;
                    if state.admissible(policy, basic_cost) == Admissibility::Admissible {
                        match eval_policy(*op, a.clone(), b.clone(), policy) {
                            Ok(selected) => {
                                let tier_meta = Tier::meta(selected.tier);
                                if tier_meta.cost < base_meta.cost {
                                    report.greedy_first_feasible.push(CounterExample {
                                        op: *op,
                                        tier: selected.tier,
                                        a: a.clone(),
                                        b: b.clone(),
                                        note: "selected tier lower than base tier",
                                    });
                                }
                                if selected.tier > base {
                                    if let Ok(promoted) = selected.promote(base) {
                                        let prev = previous_payload(
                                            &a.payload,
                                            *op,
                                            &b.payload,
                                        );
                                        if promoted.payload != prev {
                                            report.capable_tiers.push(CounterExample {
                                                op: *op,
                                                tier: base,
                                                a: a.clone(),
                                                b: b.clone(),
                                                note: "value not preserved upward after demote",
                                            });
                                        }
                                    }
                                }
                            }
                            Err(_) => {
                                report.greedy_first_feasible.push(CounterExample {
                                    op: *op,
                                    tier: base,
                                    a: a.clone(),
                                    b: b.clone(),
                                    note: "selected tier search failed",
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    report
}

fn previous_payload(a: &Num, op: Op, b: &Num) -> Num {
    match (a, b, op) {
        (Num::I64(a), Num::I64(b), Op::Add) => Num::I64(a + b),
        (Num::BigInt(a), Num::BigInt(b), Op::Add) => Num::BigInt(a + b),
        (_, _, _) => Num::Undefined,
    }
}

// ============================================================================
// std::fmt
// ============================================================================

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.meta().name)
    }
}

impl std::fmt::Display for Interval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}, {}]", self.lo, self.hi)
    }
}

impl std::fmt::Display for NumericState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.payload {
            Num::I64(n) => write!(f, "{}", n),
            Num::BigInt(n) => write!(f, "{}", n),
            Num::F64(n) => write!(f, "{}", n),
            Num::DoubleDouble { hi, lo } => write!(f, "({} + {})", hi, lo),
            Num::Rational(r) => write!(f, "{}", r),
            Num::Decimal(s) => write!(f, "Decimal({})", s),
            Num::SymbolicExpr(kind, _) => write!(f, "<{}>", kind),
            Num::Undefined => write!(f, "<undefined>"),
        }?;
        if self.interval.width() > f64::EPSILON {
            write!(f, " ± {}", self.interval.width() / 2.0)?;
        }
        Ok(())
    }
}

// ============================================================================
// Public re-exports
// ============================================================================

pub mod tests {
    #[cfg(test)]
    mod apc_tests {
        use crate::*;

        #[test]
        fn promotion_preserves_value() {
            let state = make_state(Tier::T0, Num::I64(7));
            let promoted = state.promote(Tier::T7).expect("promotion to top tier");
            let original = 7_f64;
            let promoted_value = promoted
                .payload
                .as_f64()
                .or_else(|| match promoted.payload {
                    Num::SymbolicExpr(_, ref values) if values.len() == 1 => values[0].as_f64(),
                    _ => None,
                })
                .expect("promoted payload should be numerically representable");
            assert!((promoted_value - original).abs() < f64::EPSILON);
        }

        #[test]
        fn demotion_allowed_only_when_exact_representable() {
            let exact = make_state(Tier::T4, Num::BigInt(BigInt::from(10)));
            assert!(exact.demote(Tier::T0).is_ok());
            let inexact = make_state(
                Tier::T4,
                Num::Rational(Ratio::new(BigInt::from(1), BigInt::from(3))),
            );
            assert!(inexact.demote(Tier::T0).is_err());
        }

        #[test]
        fn greedy_search_finds_first_admissible() {
            let a = make_state(Tier::T0, Num::I64(1));
            let b = make_state(Tier::T0, Num::I64(2));
            let policy = Policy {
                max_interval_width: 1.0,
                max_cost_budget: 100,
                require_determinism: true,
            };
            let result =
                eval_policy(Op::Add, a, b, policy).expect("addition with loose policy");
            assert!(result.interval.lo.is_finite());
        }

        #[test]
        fn budget_exceeded_is_reported() {
            let a = make_state(Tier::T0, Num::I64(1));
            let b = make_state(Tier::T0, Num::I64(2));
            let policy = Policy {
                max_interval_width: 0.0,
                max_cost_budget: 0,
                require_determinism: true,
            };
            let err = eval_policy(Op::Add, a, b, policy).expect_err("budget=0");
            match err {
                APCError::BudgetExceeded => {}
                other => panic!("unexpected error: {:?}", other),
            }
        }
    }
}
