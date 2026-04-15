
#[derive(Clone, Debug)]
pub struct Genome {
    pub rsi_period: usize,
    pub buy_threshold: f64,
    pub sell_threshold: f64,
}

impl Genome {
    pub fn random() -> Self {
        let mut rng = fastrand::Rng::new();

        let buy_threshold = rng.f64() * 30.0 + 10.0;
        let sell_threshold = rng.f64() * 30.0 + 60.0;

        let mut genome = Self {
            rsi_period: rng.usize(5..30),
            buy_threshold,
            sell_threshold,
        };

        genome.normalize();
        genome
    }

    pub fn normalize(&mut self) {
        self.rsi_period = self.rsi_period.clamp(5, 29);
        self.buy_threshold = self.buy_threshold.clamp(5.0, 45.0);
        self.sell_threshold = self.sell_threshold.clamp(55.0, 95.0);

        if self.buy_threshold >= self.sell_threshold - 1.0 {
            let mid = (self.buy_threshold + self.sell_threshold) / 2.0;
            self.buy_threshold = (mid - 5.0).max(5.0);
            self.sell_threshold = (mid + 5.0).min(95.0);
        }
    }
}

pub fn genome_to_strategy(genome: &Genome) -> (String, String) {
    let buy = format!("rsi < {:.2}", genome.buy_threshold);
    let sell = format!("rsi > {:.2}", genome.sell_threshold);
    (buy, sell)
}
