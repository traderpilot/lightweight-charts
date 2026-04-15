use crate::indicator::ast::*;
use crate::indicator::dsl::*;
use std::collections::HashSet;

#[derive(Debug)]
pub enum CompileError {
    DuplicateOutput(String),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::DuplicateOutput(output) => write!(f, "Duplicate output name: {}", output),
        }
    }
}

impl std::error::Error for CompileError {}

pub fn compile(def: IndicatorDef) -> Result<CompiledIndicator, CompileError> {
    let mut ops = Vec::new();
    let mut outputs = Vec::new();
    let mut seen_outputs = HashSet::new();

    for node in def.logic {
        match node {
            LogicNode::EMA { period, field, output } => {
                if !seen_outputs.insert(output.clone()) {
                    return Err(CompileError::DuplicateOutput(output));
                }
                ops.push(Op::EMA { period, field, out: output.clone() });
                outputs.push(output);
            }
            LogicNode::RSI { period, field, output } => {
                if !seen_outputs.insert(output.clone()) {
                    return Err(CompileError::DuplicateOutput(output));
                }
                ops.push(Op::RSI { period, field, out: output.clone() });
                outputs.push(output);
            }
            LogicNode::Highest { period, field, output } => {
                if !seen_outputs.insert(output.clone()) {
                    return Err(CompileError::DuplicateOutput(output));
                }
                ops.push(Op::Highest { period, field, out: output.clone() });
                outputs.push(output);
            }
            LogicNode::Lowest { period, field, output } => {
                if !seen_outputs.insert(output.clone()) {
                    return Err(CompileError::DuplicateOutput(output));
                }
                ops.push(Op::Lowest { period, field, out: output.clone() });
                outputs.push(output);
            }
        }
    }

    Ok(CompiledIndicator {
        name: def.name,
        inputs: def.inputs.unwrap_or_default(),
        ops,
        outputs,
        signals: def.signals.unwrap_or_default(),
    })
}
