#[derive(Debug)]
pub struct SpeedHistory {
    down: Vec<u64>,
    up: Vec<u64>,
    capacity: usize,
}

impl SpeedHistory {
    pub fn new(capacity: usize) -> Self {
        Self {
            down: Vec::with_capacity(capacity),
            up: Vec::with_capacity(capacity),
            capacity: capacity.max(1),
        }
    }

    pub fn push(&mut self, download: u64, upload: u64) {
        push_capped(&mut self.down, download, self.capacity);
        push_capped(&mut self.up, upload, self.capacity);
    }

    pub fn down(&self) -> &[u64] {
        &self.down
    }

    pub fn up(&self) -> &[u64] {
        &self.up
    }

    pub fn max(&self) -> u64 {
        self.down
            .iter()
            .chain(self.up.iter())
            .copied()
            .max()
            .unwrap_or(0)
    }
}

fn push_capped(samples: &mut Vec<u64>, value: u64, capacity: usize) {
    if samples.len() == capacity {
        samples.remove(0);
    }
    samples.push(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_samples_in_order() {
        let mut history = SpeedHistory::new(3);
        history.push(1, 10);
        history.push(2, 20);
        assert_eq!(history.down(), &[1, 2]);
        assert_eq!(history.up(), &[10, 20]);
    }

    #[test]
    fn drops_oldest_sample_when_full() {
        let mut history = SpeedHistory::new(2);
        history.push(1, 0);
        history.push(2, 0);
        history.push(3, 0);
        assert_eq!(history.down(), &[2, 3]);
    }

    #[test]
    fn tracks_peak() {
        let mut history = SpeedHistory::new(4);
        history.push(5, 50);
        history.push(7, 3);
        assert_eq!(history.max(), 50);
    }
}
