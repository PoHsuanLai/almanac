//! The frozen skeletons: each interface's signatures with every method answering `NotSupported`.
//! They exist so the introspection can be generated and checked against the XML file.

mod control;
mod recall;
mod record;

pub(crate) use control::ControlSkeleton;
pub(crate) use recall::RecallSkeleton;
pub(crate) use record::RecordSkeleton;
