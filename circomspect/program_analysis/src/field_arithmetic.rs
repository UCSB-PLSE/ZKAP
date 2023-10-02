use log::debug;

use program_structure::cfg::Cfg;
use program_structure::report_code::ReportCode;
use program_structure::report::{Report, ReportCollection};
use program_structure::file_definition::{FileID, FileLocation};
use program_structure::ir::*;

pub struct FieldElementArithmeticWarning {
    file_id: Option<FileID>,
    file_location: FileLocation,
}

impl FieldElementArithmeticWarning {
    pub fn into_report(self) -> Report {
        let mut report = Report::info(
            "Field element arithmetic could overflow, which may produce unexpected results."
                .to_string(),
            ReportCode::FieldElementArithmetic,
        );
        if let Some(file_id) = self.file_id {
            report.add_primary(
                self.file_location,
                file_id,
                "Field element arithmetic here.".to_string(),
            );
        }
        report
    }
}

/// Field element arithmetic in Circom may overflow, which could produce
/// unexpected results. Worst case, it may allow a malicious prover to forge
/// proofs.
pub fn find_field_element_arithmetic(cfg: &Cfg) -> ReportCollection {
    debug!("running field element arithmetic analysis pass");
    let mut reports = ReportCollection::new();
    for basic_block in cfg.iter() {
        for stmt in basic_block.iter() {
            visit_statement(stmt, &mut reports);
        }
    }
    debug!("{} new reports generated", reports.len());
    reports
}

fn visit_statement(stmt: &Statement, reports: &mut ReportCollection) {
    use Statement::*;
    match stmt {
        Declaration { dimensions, .. } => {
            for size in dimensions {
                visit_expression(size, reports);
            }
        }
        LogCall { args, .. } => {
            use LogArgument::*;
            for arg in args {
                if let Expr(value) = arg {
                    visit_expression(value, reports);
                }
            }
        }
        IfThenElse { cond, .. } => visit_expression(cond, reports),
        Substitution { rhe, .. } => visit_expression(rhe, reports),
        Return { value, .. } => visit_expression(value, reports),
        Assert { arg, .. } => visit_expression(arg, reports),
        ConstraintEquality { lhe, rhe, .. } => {
            visit_expression(lhe, reports);
            visit_expression(rhe, reports);
        }
    }
}

fn visit_expression(expr: &Expression, reports: &mut ReportCollection) {
    use Expression::*;
    match expr {
        InfixOp { meta, infix_op, .. } if may_overflow(infix_op) => {
            reports.push(build_report(meta));
        }
        InfixOp { lhe, rhe, .. } => {
            visit_expression(lhe, reports);
            visit_expression(rhe, reports);
        }
        PrefixOp { rhe, .. } => {
            visit_expression(rhe, reports);
        }
        SwitchOp { cond, if_true, if_false, .. } => {
            visit_expression(cond, reports);
            visit_expression(if_true, reports);
            visit_expression(if_false, reports);
        }
        Call { args, .. } => {
            for arg in args {
                visit_expression(arg, reports);
            }
        }
        InlineArray { values, .. } => {
            for value in values {
                visit_expression(value, reports);
            }
        }
        Access { access, .. } => {
            for index in access {
                if let AccessType::ArrayAccess(index) = index {
                    visit_expression(index, reports);
                }
            }
        }
        Update { access, rhe, .. } => {
            for index in access {
                if let AccessType::ArrayAccess(index) = index {
                    visit_expression(index, reports);
                }
            }
            visit_expression(rhe, reports);
        }
        Number(_, _) | Variable { .. } | Phi { .. } => (),
    }
}

fn is_arithmetic_infix_op(op: &ExpressionInfixOpcode) -> bool {
    use ExpressionInfixOpcode::*;
    matches!(
        op,
        Mul | Div | Add | Sub | Pow | IntDiv | Mod | ShiftL | ShiftR | BitOr | BitAnd | BitXor
    )
}

fn may_overflow(op: &ExpressionInfixOpcode) -> bool {
    use ExpressionInfixOpcode::*;
    // Note that right-shift may overflow if the shift is less than 0.
    is_arithmetic_infix_op(op) && !matches!(op, IntDiv | Mod | BitOr | BitAnd | BitXor)
}

fn build_report(meta: &Meta) -> Report {
    FieldElementArithmeticWarning { file_id: meta.file_id(), file_location: meta.file_location() }
        .into_report()
}

#[cfg(test)]
mod tests {
    use parser::parse_definition;
    use program_structure::{cfg::IntoCfg, constants::Curve};

    use super::*;

    #[test]
    fn test_field_arithmetic() {
        let src = r#"
            function f(a) {
                var b[2] = [0, 1];
                var c = b[a + 1];
                return a + b[1] + c;
            }
        "#;
        validate_reports(src, 2);
    }

    fn validate_reports(src: &str, expected_len: usize) {
        // Build CFG.
        let mut reports = ReportCollection::new();
        let cfg = parse_definition(src)
            .unwrap()
            .into_cfg(&Curve::default(), &mut reports)
            .unwrap()
            .into_ssa()
            .unwrap();
        assert!(reports.is_empty());

        // Generate report collection.
        let reports = find_field_element_arithmetic(&cfg);

        assert_eq!(reports.len(), expected_len);
    }
}
