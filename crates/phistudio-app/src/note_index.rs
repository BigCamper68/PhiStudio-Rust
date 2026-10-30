//! Revision-scoped interval index. Results retain raw source/paint order.
use phistudio_core::{Note, NoteKind};

pub(crate) struct NoteIndex {
    order: Vec<usize>,
    starts: Vec<f64>,
    ends: Vec<f64>,
    max_end: Vec<f64>,
    size: usize,
}
impl NoteIndex {
    pub fn new(notes: &[(usize, Note)]) -> Self {
        let mut order: Vec<_> = (0..notes.len()).collect();
        order.sort_unstable_by_key(|&i| (notes[i].1.start_time, i));
        let starts: Vec<_> = order
            .iter()
            .map(|&i| notes[i].1.start_time.as_f64())
            .collect();
        let ends: Vec<_> = order
            .iter()
            .map(|&i| {
                let n = &notes[i].1;
                if n.kind == NoteKind::Hold {
                    n.end_time.as_f64().max(n.start_time.as_f64())
                } else {
                    n.start_time.as_f64()
                }
            })
            .collect();
        let size = order.len().max(1).next_power_of_two();
        let mut max_end = vec![f64::NEG_INFINITY; size * 2];
        max_end[size..size + ends.len()].copy_from_slice(&ends);
        for i in (1..size).rev() {
            max_end[i] = max_end[i * 2].max(max_end[i * 2 + 1]);
        }
        Self {
            order,
            starts,
            ends,
            max_end,
            size,
        }
    }
    pub fn query(&self, first: f64, last: f64) -> Vec<usize> {
        if self.starts.first().is_some_and(|&v| first <= v)
            && self.starts.last().is_some_and(|&v| last >= v)
        {
            return (0..self.order.len()).collect();
        }
        let mut out = vec![];
        if first <= last {
            let limit = self.starts.partition_point(|&v| v <= last);
            self.visit(1, 0, self.size, limit, first, &mut out);
            out.sort_unstable();
        }
        out
    }
    fn visit(
        &self,
        node: usize,
        lo: usize,
        hi: usize,
        limit: usize,
        first: f64,
        out: &mut Vec<usize>,
    ) {
        if lo >= limit || self.max_end[node] < first {
            return;
        }
        if hi - lo == 1 {
            if self.ends[lo] >= first {
                out.push(self.order[lo]);
            }
        } else {
            let mid = (lo + hi) / 2;
            self.visit(node * 2, lo, mid, limit, first, out);
            self.visit(node * 2 + 1, mid, hi, limit, first, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phistudio_core::Beat;
    #[test]
    fn queries_match_full_scan_with_unsorted_overlaps_and_long_holds() {
        let notes: Vec<_> = (0..4000)
            .map(|i| {
                let start = Beat::from_grid(((i * 7919) % 4000) as f64 / 4. - 100., 4).unwrap();
                let hold = i % 7 == 0;
                (
                    i,
                    Note {
                        kind: if hold { NoteKind::Hold } else { NoteKind::Tap },
                        start_time: start,
                        end_time: if hold {
                            start.checked_add(Beat::new(600, 0, 1).unwrap()).unwrap()
                        } else {
                            start
                        },
                        ..Note::default()
                    },
                )
            })
            .collect();
        let index = NoteIndex::new(&notes);
        for first in [-150., -100., 0., 23.25, 600., 900., 1600.] {
            for width in [0., 0.25, 20., 2000.] {
                let expected: Vec<_> = notes
                    .iter()
                    .enumerate()
                    .filter_map(|(i, (_, n))| {
                        let end = if n.kind == NoteKind::Hold {
                            n.end_time
                        } else {
                            n.start_time
                        };
                        (n.start_time.as_f64() <= first + width && end.as_f64() >= first)
                            .then_some(i)
                    })
                    .collect();
                assert_eq!(index.query(first, first + width), expected);
            }
        }
        assert!(NoteIndex::new(&[]).query(0., 1.).is_empty());
    }
}
