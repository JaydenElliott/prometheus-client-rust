use crate::counter::{Atomic, Counter};
use crate::family::MetricFamily;
use crate::histogram::Histogram;
use crate::label::LabelSet;
use crate::registry::Registry;
use std::borrow::Cow;
use std::io::Write;

pub fn encode<W, M, S>(writer: &mut W, registry: &Registry<M>) -> Result<(), std::io::Error>
where
    W: Write,
    M: EncodeMetric,
    S: Encode,
{
    for (desc, metric) in registry.iter() {
        writer.write(b"# HELP ")?;
        writer.write(desc.name().as_bytes())?;
        writer.write(b" ")?;
        writer.write(desc.help().as_bytes())?;
        writer.write(b"\n")?;

        writer.write(b"# TYPE ")?;
        writer.write(desc.name().as_bytes())?;
        writer.write(b" ")?;
        writer.write(desc.m_type().as_bytes())?;
        writer.write(b"\n")?;

        let encoder = Encoder {
            writer: writer,
            name: &desc.name(),
            labels: None::<&()>,
        };

        metric.encode(encoder)?;
    }

    writer.write("# EOF\n".as_bytes())?;

    Ok(())
}

pub struct Encoder<'a, 'b, W, S> {
    writer: &'a mut W,
    name: &'a str,
    labels: Option<&'b S>,
    // TODO: Check if label brackets are open.
}

// TODO: How about each function returns a new encoder that only allows encoding
// what is allowed next?
impl<'a, 'b, W: Write, S: Encode> Encoder<'a, 'b, W, S> {
    fn encode_suffix(&mut self, suffix: &'static str) -> Result<BucketEncoder<W>, std::io::Error> {
        self.writer.write(self.name.as_bytes())?;
        self.writer.write("_".as_bytes())?;
        self.writer.write(suffix.as_bytes()).map(|_| ())?;

        self.encode_labels()
    }

    fn no_suffix(&mut self) -> Result<BucketEncoder<W>, std::io::Error> {
        self.writer.write(self.name.as_bytes())?;

        self.encode_labels()
    }

    pub(self) fn encode_labels(&mut self) -> Result<BucketEncoder<W>, std::io::Error> {
        if let Some(labels) = &self.labels {
            self.writer.write("{".as_bytes())?;
            labels.encode(self.writer)?;

            Ok(BucketEncoder {
                opened_curly_brackets: true,
                writer: self.writer,
            })
        } else {
            Ok(BucketEncoder {
                opened_curly_brackets: false,
                writer: self.writer,
            })
        }
    }

    fn with_label_set<'c, 'd, NewLabelSet>(
        &'c mut self,
        label_set: &'d NewLabelSet,
    ) -> Encoder<'c, 'd, W, NewLabelSet> {
        debug_assert!(self.labels.is_none());

        Encoder {
            writer: self.writer,
            name: self.name,
            labels: Some(label_set),
        }
    }
}

#[must_use]
pub struct BucketEncoder<'a, W> {
    writer: &'a mut W,
    opened_curly_brackets: bool,
}

impl<'a, W: Write> BucketEncoder<'a, W> {
    fn encode_bucket<K: Encode, V: Encode>(
        &mut self,
        key: K,
        value: V,
    ) -> Result<ValueEncoder<W>, std::io::Error> {
        if self.opened_curly_brackets {
            self.writer.write(", ".as_bytes())?;
        } else {
            self.writer.write("{".as_bytes())?;
        }

        key.encode(self.writer)?;
        self.writer.write("=\"".as_bytes())?;
        value.encode(self.writer)?;
        self.writer.write("\"}".as_bytes())?;

        Ok(ValueEncoder {
            writer: self.writer,
        })
    }

    fn no_bucket(&mut self) -> Result<ValueEncoder<W>, std::io::Error> {
        if self.opened_curly_brackets {
            self.writer.write("}".as_bytes())?;
        }
        Ok(ValueEncoder {
            writer: self.writer,
        })
    }
}

#[must_use]
pub struct ValueEncoder<'a, W> {
    writer: &'a mut W,
}

impl<'a, W: Write> ValueEncoder<'a, W> {
    fn encode_value<V: Encode>(&mut self, v: V) -> Result<(), std::io::Error> {
        self.writer.write(" ".as_bytes())?;
        v.encode(self.writer)?;
        self.writer.write("\n".as_bytes())?;
        Ok(())
    }
}

pub trait EncodeMetric {
    fn encode<'a, 'b, W: Write, S: Encode>(
        &self,
        encoder: Encoder<'a, 'b, W, S>,
    ) -> Result<(), std::io::Error>;
}

pub trait Encode {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), std::io::Error>;
}

impl Encode for () {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), std::io::Error> {
        Ok(())
    }
}

impl Encode for f64 {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), std::io::Error> {
        // TODO: Can we do better?
        writer.write(self.to_string().as_bytes())?;
        Ok(())
    }
}

impl Encode for u64 {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), std::io::Error> {
        // TODO: Can we do better?
        writer.write(self.to_string().as_bytes())?;
        Ok(())
    }
}

impl Encode for &str {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), std::io::Error> {
        // TODO: Can we do better?
        writer.write(self.as_bytes())?;
        Ok(())
    }
}

impl Encode for Vec<(String, String)> {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), std::io::Error> {
        if self.is_empty() {
            return Ok(());
        }

        let mut iter = self.iter().peekable();
        while let Some((name, value)) = iter.next() {
            writer.write(name.as_bytes())?;
            writer.write(b"=\"")?;
            writer.write(value.as_bytes())?;
            writer.write(b"\"")?;

            if iter.peek().is_some() {
                writer.write(b",")?;
            }
        }

        Ok(())
    }
}

impl<A> EncodeMetric for Counter<A>
where
    A: Atomic,
    <A as Atomic>::Number: Encode,
{
    fn encode<'a, 'b, W: Write, S: Encode>(
        &self,
        mut encoder: Encoder<'a, 'b, W, S>,
    ) -> Result<(), std::io::Error> {
        encoder
            .encode_suffix("total")?
            .no_bucket()?
            .encode_value(self.get())?;

        Ok(())
    }
}

impl<S, M> EncodeMetric for MetricFamily<S, M>
where
    // TODO: Does S need to be Clone?
    S: Clone + LabelSet + std::hash::Hash + Eq + Encode,
    M: Default + EncodeMetric,
{
    fn encode<'a, 'b, W: Write, NoneLabelSet: Encode>(
        &self,
        mut encoder: Encoder<'a, 'b, W, NoneLabelSet>,
    ) -> Result<(), std::io::Error> {
        let guard = self.read();
        let mut iter = guard.iter();
        while let Some((label_set, m)) = iter.next() {
            let encoder = encoder.with_label_set(label_set);
            m.encode(encoder)?;
        }
        Ok(())
    }
}

impl EncodeMetric for Histogram {
    fn encode<W: Write, NoneLabelSet: Encode>(
        &self,
        mut encoder: Encoder<W, NoneLabelSet>,
    ) -> Result<(), std::io::Error> {
        // TODO: Acquire lock for the entire time, not one by one.
        encoder
            .encode_suffix("sum")?
            .no_bucket()?
            .encode_value(self.sum())?;
        encoder
            .encode_suffix("count")?
            .no_bucket()?
            .encode_value(self.count())?;

        for (upper_bound, count) in self.buckets().iter() {
            let label = (
                "le".to_string(),
                if *upper_bound == f64::MAX {
                    "+Inf".to_string()
                } else {
                    upper_bound.to_string()
                },
            );

            let bucket_key = if *upper_bound == f64::MAX {
                "+Inf".to_string()
            } else {
                upper_bound.to_string()
            };

            encoder
                .encode_suffix("bucket")?
                .encode_bucket("le", bucket_key.as_str())?
                .encode_value(*count)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::counter::Counter;
    use crate::registry::Descriptor;
    use pyo3::{prelude::*, types::PyModule};
    use std::sync::atomic::AtomicU64;

    #[test]
    fn encode_counter() {
        let mut registry = Registry::new();
        let counter = Counter::<AtomicU64>::new();
        registry.register(
            Descriptor::new("counter", "My counter", "my_counter"),
            counter.clone(),
        );

        let mut encoded = Vec::new();

        encode::<_, _, Vec<(String, String)>>(&mut encoded, &registry).unwrap();

        parse_with_python_client(String::from_utf8(encoded).unwrap());
    }

    #[test]
    fn encode_counter_family() {
        let mut registry = Registry::new();
        let family = MetricFamily::<Vec<(String, String)>, Counter<AtomicU64>>::new();
        registry.register(
            Descriptor::new("counter", "My counter family", "my_counter_family"),
            family.clone(),
        );

        family
            .get_or_create(&vec![("method".to_string(), "GET".to_string())])
            .inc();

        let mut encoded = Vec::new();

        encode::<_, _, Vec<(String, String)>>(&mut encoded, &registry).unwrap();

        parse_with_python_client(String::from_utf8(encoded).unwrap());
    }

    #[test]
    fn encode_histogram() {
        let mut registry = Registry::new();
        let histogram = Histogram::new(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]);
        registry.register(
            Descriptor::new("histogram", "My histogram", "my_histogram"),
            histogram.clone(),
        );
        histogram.observe(1.0);

        let mut encoded = Vec::new();

        encode::<_, _, Vec<(String, String)>>(&mut encoded, &registry).unwrap();

        parse_with_python_client(String::from_utf8(encoded).unwrap());
    }

    fn parse_with_python_client(input: String) {
        println!("{:?}", input);
        Python::with_gil(|py| {
            let parser = PyModule::from_code(
                py,
                r#"
from prometheus_client.openmetrics.parser import text_string_to_metric_families

def parse(input):
    families = text_string_to_metric_families(input)
    list(families)
"#,
                "parser.py",
                "parser",
            )
            .map_err(|e| e.to_string())
            .unwrap();
            parser
                .call1("parse", (input,))
                .map_err(|e| e.to_string())
                .unwrap();
        })
    }
}
