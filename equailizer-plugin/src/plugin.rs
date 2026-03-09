use crate::context::{Context, HandlerResult};
use crate::events::{BatchCreated, BatchReconciled, CommandError, ReconcileAllComplete};

pub trait Plugin {
    fn name(&self) -> &str;
    fn version(&self) -> &str;

    fn on_batch_created(&mut self, ctx: &Context, event: &BatchCreated) -> HandlerResult {
        let _ = (ctx, event);
        Ok(())
    }

    fn on_batch_reconciled(&mut self, ctx: &Context, event: &BatchReconciled) -> HandlerResult {
        let _ = (ctx, event);
        Ok(())
    }

    fn on_command_error(&mut self, ctx: &Context, event: &CommandError) -> HandlerResult {
        let _ = (ctx, event);
        Ok(())
    }

    fn on_reconcile_all_complete(
        &mut self,
        ctx: &Context,
        event: &ReconcileAllComplete,
    ) -> HandlerResult {
        let _ = (ctx, event);
        Ok(())
    }

    fn on_shutdown(&mut self) -> HandlerResult {
        Ok(())
    }
}
