mod miri {

    use indoc::indoc;

    use crate::{TestAction, run_test_actions};

    #[test]
    fn finalization_registry_simple() {
        run_test_actions([
            TestAction::run(indoc! {r#"
            let counter = 0;
            const registry = new FinalizationRegistry(() => {
                counter++;
            });

            registry.register(["foo"]);
        "#}),
            TestAction::assert_eq("counter", 0),
            TestAction::inspect_context(|_| boa_gc::force_collect()),
            TestAction::inspect_context(|ctx| ctx.run_jobs().unwrap()),
            // Callback should run at least once
            TestAction::assert_eq("counter", 1),
        ]);
    }

    #[test]
    fn finalization_registry_unregister() {
        run_test_actions([
            TestAction::run(indoc! {r#"
            let counter = 0;
            const registry = new FinalizationRegistry(() => {
                counter++;
            });

            {
                let array = ["foo"];
                registry.register(array, undefined, array);
                registry.unregister(array);
            }

        "#}),
            TestAction::assert_eq("counter", 0),
            TestAction::inspect_context(|_| boa_gc::force_collect()),
            TestAction::inspect_context(|ctx| ctx.run_jobs().unwrap()),
            // Callback shouldn't run
            TestAction::assert_eq("counter", 0),
        ]);
    }

    #[test]
    fn finalization_registry_held_value_handover() {
        run_test_actions([
            TestAction::run(indoc! {r#"
            let counter = 0;
            const registry = new FinalizationRegistry((value) => {
                counter += value.increment;
            });

            registry.register(["foo"], { increment: 5 });
        "#}),
            TestAction::assert_eq("counter", 0),
            TestAction::inspect_context(|_| boa_gc::force_collect()),
            TestAction::inspect_context(|ctx| ctx.run_jobs().unwrap()),
            // Registry should handover the held value as argument
            TestAction::assert_eq("counter", 5),
        ]);
    }

    #[test]
    fn finalization_registry_unrelated_unregister_token() {
        run_test_actions([
            TestAction::run(indoc! {r#"
            let counter = 0;

            const registry = new FinalizationRegistry((value) => {
                counter += 1;
            });

            registry.register(["foo"], undefined, {});
            registry.unregister({});
        "#}),
            TestAction::assert_eq("counter", 0),
            TestAction::inspect_context(|_| boa_gc::force_collect()),
            TestAction::inspect_context(|ctx| ctx.run_jobs().unwrap()),
            // Object should not have been unregistered if the token is not the correct one
            TestAction::assert_eq("counter", 1),
        ]);
    }

    #[test]
    fn finalization_registry_waiter_survives_job_drain() {
        run_test_actions([
            TestAction::run(indoc! {r#"
                let counter = 0;
                const registry = new FinalizationRegistry(() => counter++);
                let target = {};
                registry.register(target);
                target = null;
            "#}),
            // Drain once before collection. The next drain must still have a waiter.
            TestAction::inspect_context(|ctx| ctx.run_jobs().unwrap()),
            TestAction::assert_eq("counter", 0),
            TestAction::inspect_context(|_| boa_gc::force_collect()),
            TestAction::inspect_context(|ctx| ctx.run_jobs().unwrap()),
            TestAction::assert_eq("counter", 1),
        ]);
    }

    #[test]
    fn finalization_registry_waiters_handle_multiple_registries_and_drains() {
        run_test_actions([
            TestAction::run(indoc! {r#"
                let firstCount = 0;
                let secondCount = 0;
                const first = new FinalizationRegistry(() => firstCount++);
                const second = new FinalizationRegistry(() => secondCount++);
                let firstTarget = {};
                let secondTarget = {};
                first.register(firstTarget);
                second.register(secondTarget);
                firstTarget = null;
                secondTarget = null;
            "#}),
            TestAction::inspect_context(|ctx| ctx.run_jobs().unwrap()),
            TestAction::inspect_context(|_| boa_gc::force_collect()),
            TestAction::inspect_context(|ctx| ctx.run_jobs().unwrap()),
            TestAction::assert_eq("firstCount", 1),
            TestAction::assert_eq("secondCount", 1),
            TestAction::run(indoc! {r#"
                let firstTargetAgain = {};
                let secondTargetAgain = {};
                first.register(firstTargetAgain);
                second.register(secondTargetAgain);
                firstTargetAgain = null;
                secondTargetAgain = null;
            "#}),
            TestAction::inspect_context(|ctx| ctx.run_jobs().unwrap()),
            TestAction::inspect_context(|_| boa_gc::force_collect()),
            TestAction::inspect_context(|ctx| ctx.run_jobs().unwrap()),
            TestAction::assert_eq("firstCount", 2),
            TestAction::assert_eq("secondCount", 2),
        ]);
    }

    #[test]
    fn finalization_registry_waiter_does_not_keep_dead_registry_alive() {
        run_test_actions([
            TestAction::run(indoc! {r#"
                let counter = 0;
                {
                    const registry = new FinalizationRegistry(() => counter++);
                    let target = {};
                    registry.register(target);
                    target = null;
                }
            "#}),
            TestAction::inspect_context(|ctx| ctx.run_jobs().unwrap()),
            TestAction::inspect_context(|_| boa_gc::force_collect()),
            TestAction::inspect_context(|ctx| ctx.run_jobs().unwrap()),
            TestAction::inspect_context(|ctx| ctx.run_jobs().unwrap()),
            TestAction::assert_eq("counter", 0),
        ]);
    }
}
