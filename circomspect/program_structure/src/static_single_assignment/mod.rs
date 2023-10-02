//! This module implements a generic conversion into single-static assignment
//! form.
pub mod dominator_tree;
pub mod errors;
pub mod traits;

use log::trace;

use dominator_tree::DominatorTree;
use errors::SSAResult;
use traits::*;

/// Insert a dummy phi statement in block `j`, for each variable written in block
/// `i`, if `j` is in the dominance frontier of `i`.
pub fn insert_phi_statements<Cfg: SSAConfig>(
    basic_blocks: &mut [Cfg::BasicBlock],
    dominator_tree: &DominatorTree<Cfg::BasicBlock>,
    env: &mut Cfg::Environment,
) {
    // Insert phi statements at the dominance frontier of each block.
    let mut work_list: Vec<Index> = (0..basic_blocks.len()).collect();
    while let Some(current_index) = work_list.pop() {
        let variables_written = {
            let current_block = &basic_blocks[current_index];
            current_block.variables_written().clone()
        };
        if variables_written.is_empty() {
            trace!("basic block {current_index} does not write any variables");
            continue;
        }
        trace!(
            "dominance frontier for block {current_index} is {:?}",
            dominator_tree.get_dominance_frontier(current_index)
        );
        for frontier_index in dominator_tree.get_dominance_frontier(current_index) {
            let frontier_block = &mut basic_blocks[frontier_index];
            for var in &variables_written {
                if !frontier_block.has_phi_statement(var) {
                    // If a phi statement was added to the block we need to
                    // re-add the block to the work list.
                    frontier_block.insert_phi_statement(var, env);
                    work_list.push(frontier_index);
                }
            }
        }
    }
}

/// Traverses the dominator tree in pre-order and for each block, the function
///
/// 1. Renames all variables to SSA form, keeping track of the current
///    version of each variable.
/// 2. Updates phi expression arguments in each successor of the current
///    block, adding the correct versioned arguments to the expression.
pub fn insert_ssa_variables<'a, Cfg: SSAConfig>(
    basic_blocks: &'a mut [Cfg::BasicBlock],
    dominator_tree: &DominatorTree<Cfg::BasicBlock>,
    env: &mut Cfg::Environment,
) -> SSAResult<()> {
    insert_ssa_variables_impl::<Cfg>(0, basic_blocks, dominator_tree, env)?;
    Ok(())
}

fn insert_ssa_variables_impl<Cfg: SSAConfig>(
    current_index: Index,
    basic_blocks: &mut [Cfg::BasicBlock],
    dominator_tree: &DominatorTree<Cfg::BasicBlock>,
    env: &mut Cfg::Environment,
) -> SSAResult<()> {
    // 1. Update variables in current block.
    let successors = {
        let current_block =
            basic_blocks.get_mut(current_index).expect("invalid block index during SSA generation");
        current_block.insert_ssa_variables(env)?;
        current_block.successors().clone()
    };
    // 2. Update phi statements in successor blocks.
    for successor_index in successors {
        let successor_block = basic_blocks
            .get_mut(successor_index)
            .expect("invalid block index during SSA generation");
        successor_block.update_phi_statements(env);
    }
    // 3. Update dominator tree successors recursively.
    for successor_index in dominator_tree.get_dominator_successors(current_index) {
        env.add_variable_scope();
        insert_ssa_variables_impl::<Cfg>(successor_index, basic_blocks, dominator_tree, env)?;
        env.remove_variable_scope();
    }
    Ok(())
}
