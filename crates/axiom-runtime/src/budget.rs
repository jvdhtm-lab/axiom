//! Runtime-owned budgets for agent calls.
//!
//! An agent declaration carries `limits` (`max_calls`, `timeout`,
//! `max_output_bytes`). The runtime enforces them; agents cannot raise or reset
//! them. A [`BudgetLedger`] tracks spend and reports exhaustion so iteration
//! stops deterministically instead of looping on provider instability.

use std::time::Duration;

/// Limits an agent declaration may set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budgets {
    /// Model calls allowed across the whole run.
    pub max_calls: u32,
    /// Output bytes allowed across the whole run.
    pub max_output_bytes: u64,
    /// Wall-clock allowance (soft: checked between attempts).
    pub timeout: Duration,
}

impl Default for Budgets {
    fn default() -> Self {
        Budgets {
            max_calls: 8,
            max_output_bytes: 1_000_000,
            timeout: Duration::from_secs(120),
        }
    }
}

impl Budgets {
    pub fn with_max_calls(mut self, max_calls: u32) -> Self {
        self.max_calls = max_calls;
        self
    }

    pub fn with_max_output_bytes(mut self, max_output_bytes: u64) -> Self {
        self.max_output_bytes = max_output_bytes;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// Why iteration stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetExhausted {
    /// No model calls remain.
    Calls,
    /// The output-byte cap would be exceeded.
    OutputBytes,
    /// The wall-clock allowance elapsed.
    Timeout,
}

impl BudgetExhausted {
    pub fn as_str(&self) -> &'static str {
        match self {
            BudgetExhausted::Calls => "max_calls",
            BudgetExhausted::OutputBytes => "max_output_bytes",
            BudgetExhausted::Timeout => "timeout",
        }
    }
}

/// Spend accounting for one [`Budgets`].
#[derive(Debug, Clone)]
pub struct BudgetLedger {
    budgets: Budgets,
    calls: u32,
    output_bytes: u64,
    elapsed: Duration,
}

impl BudgetLedger {
    pub fn new(budgets: Budgets) -> Self {
        BudgetLedger {
            budgets,
            calls: 0,
            output_bytes: 0,
            elapsed: Duration::ZERO,
        }
    }

    pub fn budgets(&self) -> Budgets {
        self.budgets
    }

    pub fn calls(&self) -> u32 {
        self.calls
    }

    pub fn output_bytes(&self) -> u64 {
        self.output_bytes
    }

    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Whether a further call producing `bytes` output is allowed.
    pub fn allows(&self, bytes: u64) -> Result<(), BudgetExhausted> {
        if self.calls >= self.budgets.max_calls {
            return Err(BudgetExhausted::Calls);
        }
        if self.output_bytes.saturating_add(bytes) > self.budgets.max_output_bytes {
            return Err(BudgetExhausted::OutputBytes);
        }
        if self.elapsed >= self.budgets.timeout {
            return Err(BudgetExhausted::Timeout);
        }
        Ok(())
    }

    /// Charge one call of `bytes` output, failing closed if disallowed.
    pub fn spend_call(&mut self, bytes: u64) -> Result<(), BudgetExhausted> {
        self.allows(bytes)?;
        self.calls += 1;
        self.output_bytes = self.output_bytes.saturating_add(bytes);
        Ok(())
    }

    /// Add elapsed wall-clock time (checked by the caller between attempts).
    pub fn spend_time(&mut self, elapsed: Duration) {
        self.elapsed = self.elapsed.saturating_add(elapsed);
    }

    /// Whether any budget is already exhausted.
    pub fn exhausted(&self) -> Option<BudgetExhausted> {
        self.allows(0).err()
    }
}
