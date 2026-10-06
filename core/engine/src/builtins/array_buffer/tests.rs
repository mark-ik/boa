use crate::object::JsArrayBuffer;
use crate::{TestAction, run_test_actions};

#[cfg(any(feature = "experimental", feature = "array-buffer-transfer"))]
use crate::{Context, Source, context::ContextBuilder, context::HostHooks};

#[cfg(any(feature = "experimental", feature = "array-buffer-transfer"))]
struct TransferLimitHooks;

#[cfg(any(feature = "experimental", feature = "array-buffer-transfer"))]
impl HostHooks for TransferLimitHooks {
    fn max_buffer_size(&self, _context: &mut Context) -> u64 {
        4
    }
}

#[cfg(any(feature = "experimental", feature = "array-buffer-transfer"))]
#[test]
fn transfer_apis_are_exposed_when_enabled() {
    run_test_actions([
        TestAction::assert("typeof ArrayBuffer.prototype.transfer === 'function'"),
        TestAction::assert("typeof ArrayBuffer.prototype.transferToFixedLength === 'function'"),
        TestAction::assert(
            "Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'detached').get !== undefined",
        ),
    ]);
}

#[cfg(all(feature = "array-buffer-transfer", not(feature = "experimental")))]
#[test]
fn transfer_feature_does_not_enable_unrelated_experimental_apis() {
    run_test_actions([TestAction::assert("typeof Atomics.pause === 'undefined'")]);
}

#[cfg(any(feature = "experimental", feature = "array-buffer-transfer"))]
#[test]
fn transfer_copies_resizes_and_detaches() {
    run_test_actions([
        TestAction::run(
            r#"
            const source = new ArrayBuffer(3);
            const sourceBytes = new Uint8Array(source);
            sourceBytes.set([5, 9, 13]);
            const moved = source.transfer();
            const movedBytes = new Uint8Array(moved);
            "#,
        ),
        TestAction::assert("moved.byteLength === 3"),
        TestAction::assert("movedBytes[0] === 5 && movedBytes[1] === 9 && movedBytes[2] === 13"),
        TestAction::assert("source.detached === true && source.byteLength === 0"),
        TestAction::run(
            r#"
            const shortSource = new ArrayBuffer(3);
            new Uint8Array(shortSource).set([2, 4, 6]);
            const longer = shortSource.transfer(5);
            "#,
        ),
        TestAction::assert("longer.byteLength === 5"),
        TestAction::assert("new Uint8Array(longer)[0] === 2 && new Uint8Array(longer)[2] === 6"),
        TestAction::assert("new Uint8Array(longer)[3] === 0 && new Uint8Array(longer)[4] === 0"),
        TestAction::run(
            r#"
            const longSource = new ArrayBuffer(4);
            new Uint8Array(longSource).set([3, 7, 11, 15]);
            const shorter = longSource.transfer(2);
            "#,
        ),
        TestAction::assert("shorter.byteLength === 2"),
        TestAction::assert("new Uint8Array(shorter)[0] === 3 && new Uint8Array(shorter)[1] === 7"),
    ]);
}

#[cfg(any(feature = "experimental", feature = "array-buffer-transfer"))]
#[test]
fn transfer_preserves_or_removes_resizability_as_requested() {
    run_test_actions([
        TestAction::run(
            r#"
            const resizableSource = new ArrayBuffer(2, { maxByteLength: 8 });
            new Uint8Array(resizableSource).set([17, 19]);
            const resizableResult = resizableSource.transfer(4);
            const fixedSource = new ArrayBuffer(2, { maxByteLength: 8 });
            new Uint8Array(fixedSource).set([23, 29]);
            const fixedResult = fixedSource.transferToFixedLength(4);
            "#,
        ),
        TestAction::assert(
            "resizableResult.resizable === true && resizableResult.maxByteLength === 8",
        ),
        TestAction::assert(
            "new Uint8Array(resizableResult)[0] === 17 && new Uint8Array(resizableResult)[1] === 19",
        ),
        TestAction::assert("fixedResult.resizable === false && fixedResult.maxByteLength === 4"),
        TestAction::assert(
            "new Uint8Array(fixedResult)[0] === 23 && new Uint8Array(fixedResult)[1] === 29",
        ),
        TestAction::assert("resizableSource.detached === true && fixedSource.detached === true"),
    ]);
}

#[cfg(any(feature = "experimental", feature = "array-buffer-transfer"))]
#[test]
fn transfer_errors_do_not_detach_the_source() {
    run_test_actions([
        TestAction::run(
            r#"
            const source = new ArrayBuffer(2);
            new Uint8Array(source).set([31, 37]);
            let invalidLengthThrew = false;
            try { source.transfer(-1); } catch (error) { invalidLengthThrew = error instanceof RangeError; }
            const capped = new ArrayBuffer(2, { maxByteLength: 4 });
            let maximumLengthThrew = false;
            try { capped.transfer(5); } catch (error) { maximumLengthThrew = error instanceof RangeError; }
            let invalidReceiverThrew = false;
            try { ArrayBuffer.prototype.transfer.call({}); } catch (error) { invalidReceiverThrew = error instanceof TypeError; }
            const detached = source.transferToFixedLength();
            let detachedSourceThrew = false;
            try { source.transfer(); } catch (error) { detachedSourceThrew = error instanceof TypeError; }
            let detachedGetterReceiverThrew = false;
            try { Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'detached').get.call({}); }
            catch (error) { detachedGetterReceiverThrew = error instanceof TypeError; }
            "#,
        ),
        TestAction::assert("invalidLengthThrew && maximumLengthThrew && invalidReceiverThrew"),
        TestAction::assert("capped.detached === false && capped.byteLength === 2"),
        TestAction::assert("source.detached === true && detached.byteLength === 2"),
        TestAction::assert(
            "new Uint8Array(detached)[0] === 31 && new Uint8Array(detached)[1] === 37",
        ),
        TestAction::assert("detachedSourceThrew && detachedGetterReceiverThrew"),
    ]);
}

#[cfg(any(feature = "experimental", feature = "array-buffer-transfer"))]
#[test]
fn transfer_respects_host_buffer_limit_without_detaching_source() {
    let mut context = ContextBuilder::new()
        .host_hooks(std::rc::Rc::new(TransferLimitHooks))
        .build()
        .unwrap();
    let result = context
        .eval(Source::from_bytes(
            r#"
            const source = new ArrayBuffer(2);
            let transferRejected = false;
            try { source.transfer(5); }
            catch (error) { transferRejected = error instanceof RangeError; }
            transferRejected && !source.detached && source.byteLength === 2
            "#,
        ))
        .unwrap();

    assert_eq!(result.as_boolean(), Some(true));
}

#[cfg(not(any(feature = "experimental", feature = "array-buffer-transfer")))]
#[test]
fn transfer_apis_are_absent_without_their_features() {
    run_test_actions([
        TestAction::assert("typeof ArrayBuffer.prototype.transfer === 'undefined'"),
        TestAction::assert("typeof ArrayBuffer.prototype.transferToFixedLength === 'undefined'"),
        TestAction::assert(
            "Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'detached') === undefined",
        ),
    ]);
}

#[test]
fn create_byte_data_block() {
    run_test_actions([TestAction::inspect_context(|context| {
        // Sunny day
        assert!(super::create_byte_data_block(100, None, context).is_ok());

        // Rainy day
        assert!(super::create_byte_data_block(u64::MAX, None, context).is_err());
    })]);
}

#[test]
fn create_shared_byte_data_block() {
    run_test_actions([TestAction::inspect_context(|context| {
        // Sunny day
        assert!(super::shared::create_shared_byte_data_block(100, context).is_ok());

        // Rainy day
        assert!(super::shared::create_shared_byte_data_block(u64::MAX, context).is_err());
    })]);
}

#[test]
fn resize() {
    run_test_actions([TestAction::inspect_context(|context| {
        let data_block = super::create_byte_data_block(100, None, context).unwrap();
        let js_arr = JsArrayBuffer::from_byte_block(data_block, context)
            .unwrap()
            .with_max_byte_length(100);
        let mut arr = js_arr.borrow_mut();

        // Sunny day
        assert_eq!(arr.data_mut().resize(50), Ok(()));

        // Rainy day
        assert!(arr.data_mut().resize(u64::MAX).is_err());
    })]);
}

#[test]
fn get_values() {
    run_test_actions([
        TestAction::run(
            r#"
            var buffer = new ArrayBuffer(12);
            var sample = new DataView(buffer, 0);

            sample.setUint8(0, 127);
            sample.setUint8(1, 255);
            sample.setUint8(2, 255);
            sample.setUint8(3, 255);
            sample.setUint8(4, 128);
            sample.setUint8(5, 0);
            sample.setUint8(6, 0);
            sample.setUint8(7, 0);
            sample.setUint8(8, 1);
            sample.setUint8(9, 0);
            sample.setUint8(10, 0);
            sample.setUint8(11, 0);
        "#,
        ),
        TestAction::assert("sample.getUint32(0, false) == 2147483647"),
        TestAction::assert("sample.getUint32(1, false) == 4294967168"),
        TestAction::assert("sample.getUint32(2, false) == 4294934528"),
        TestAction::assert("sample.getUint32(3, false) == 4286578688"),
        TestAction::assert("sample.getUint32(4, false) == 2147483648"),
        TestAction::assert("sample.getUint32(5, false) == 1"),
        TestAction::assert("sample.getUint32(6, false) == 256"),
        TestAction::assert("sample.getUint32(7, false) == 65536"),
        TestAction::assert("sample.getUint32(8, false) == 16777216"),
        TestAction::assert("sample.getUint32(0, true) == 4294967167"),
        TestAction::assert("sample.getUint32(1, true) == 2164260863"),
        TestAction::assert("sample.getUint32(2, true) == 8454143"),
        TestAction::assert("sample.getUint32(3, true) == 33023"),
        TestAction::assert("sample.getUint32(4, true) == 128"),
        TestAction::assert("sample.getUint32(5, true) == 16777216"),
        TestAction::assert("sample.getUint32(6, true) == 65536"),
        TestAction::assert("sample.getUint32(7, true) == 256"),
        TestAction::assert("sample.getUint32(8, true) == 1"),
    ]);
}

#[test]
fn sort() {
    run_test_actions([
        TestAction::run(
            r#"
            // This cmp function is needed as the harness does not support TypedArray comparison.
            function cmp(a, b) {
                return a.length === b.length && a.every((v, i) => v === b[i]);
            }

            var TypedArrayCtor = [
                Int8Array,
                Uint8Array,
                Int16Array,
                Uint16Array,
                Int32Array,
                Uint32Array,
                Float32Array,
                Float64Array,
            ];

            var descending = TypedArrayCtor.map((ctor) => new ctor([4, 3, 2, 1]).sort());
            var mixed = TypedArrayCtor.map((ctor) => new ctor([3, 4, 1, 2]).sort());
            var repeating = TypedArrayCtor.map((ctor) => new ctor([0, 1, 1, 2, 3, 3, 4]).sort());
        "#,
        ),
        // Descending
        TestAction::assert("cmp(descending[0], [1, 2, 3, 4])"),
        TestAction::assert("cmp(descending[1], [1, 2, 3, 4])"),
        TestAction::assert("cmp(descending[2], [1, 2, 3, 4])"),
        TestAction::assert("cmp(descending[3], [1, 2, 3, 4])"),
        TestAction::assert("cmp(descending[4], [1, 2, 3, 4])"),
        TestAction::assert("cmp(descending[5], [1, 2, 3, 4])"),
        TestAction::assert("cmp(descending[6], [1, 2, 3, 4])"),
        TestAction::assert("cmp(descending[7], [1, 2, 3, 4])"),
        // Mixed
        TestAction::assert("cmp(mixed[0], [1, 2, 3, 4])"),
        TestAction::assert("cmp(mixed[1], [1, 2, 3, 4])"),
        TestAction::assert("cmp(mixed[2], [1, 2, 3, 4])"),
        TestAction::assert("cmp(mixed[3], [1, 2, 3, 4])"),
        TestAction::assert("cmp(mixed[4], [1, 2, 3, 4])"),
        TestAction::assert("cmp(mixed[5], [1, 2, 3, 4])"),
        TestAction::assert("cmp(mixed[6], [1, 2, 3, 4])"),
        TestAction::assert("cmp(mixed[7], [1, 2, 3, 4])"),
        // Repeating
        TestAction::assert("cmp(repeating[0], [0, 1, 1, 2, 3, 3, 4])"),
        TestAction::assert("cmp(repeating[1], [0, 1, 1, 2, 3, 3, 4])"),
        TestAction::assert("cmp(repeating[2], [0, 1, 1, 2, 3, 3, 4])"),
        TestAction::assert("cmp(repeating[3], [0, 1, 1, 2, 3, 3, 4])"),
        TestAction::assert("cmp(repeating[4], [0, 1, 1, 2, 3, 3, 4])"),
        TestAction::assert("cmp(repeating[5], [0, 1, 1, 2, 3, 3, 4])"),
        TestAction::assert("cmp(repeating[6], [0, 1, 1, 2, 3, 3, 4])"),
        TestAction::assert("cmp(repeating[7], [0, 1, 1, 2, 3, 3, 4])"),
    ]);
}

#[test]
fn sort_negative_zero() {
    run_test_actions([
        TestAction::run(
            r#"
            // This cmp function is needed as the harness does not support TypedArray comparison.
            function cmp(a, b) {
                return a.length === b.length && a.every((v, i) => v === b[i]);
            }

            var TypedArrayCtor = [Float32Array, Float64Array];
            var negativeZero = TypedArrayCtor.map((ctor) => new ctor([1, 0, -0, 2]).sort());
            var infinities = TypedArrayCtor.map((ctor) => new ctor([3, 4, Infinity, -Infinity, 1, 2]).sort());
        "#,
        ),
        TestAction::assert("cmp(negativeZero[0], [-0, 0, 1, 2])"),
        TestAction::assert("cmp(negativeZero[1], [-0, 0, 1, 2])"),
        TestAction::assert("cmp(infinities[0], [-Infinity, 1, 2, 3, 4, Infinity])"),
        TestAction::assert("cmp(infinities[1], [-Infinity, 1, 2, 3, 4, Infinity])"),
    ]);
}

/// Tests `SharedArrayBuffer.prototype.slice` which triggers `copy_shared_to_shared`
/// (the `batched_atomic_copy_forward` path).
#[test]
fn shared_array_buffer_slice() {
    run_test_actions([
        TestAction::run(
            r#"
            var sab = new SharedArrayBuffer(16);
            var view = new Uint8Array(sab);
            for (var i = 0; i < 16; i++) view[i] = i + 1;
            var sliced = sab.slice(0);
            var result = new Uint8Array(sliced);
        "#,
        ),
        // Verify all 16 bytes copied correctly (exercises u64 batch + head/tail)
        TestAction::assert("result[0] === 1"),
        TestAction::assert("result[7] === 8"),
        TestAction::assert("result[15] === 16"),
        TestAction::assert("result.length === 16"),
    ]);
}

/// Tests `SharedArrayBuffer.prototype.slice` with a partial range and odd sizes
/// to exercise alignment edge cases in the batched copy.
#[test]
fn shared_array_buffer_slice_partial() {
    run_test_actions([
        TestAction::run(
            r#"
            var sab = new SharedArrayBuffer(20);
            var view = new Uint8Array(sab);
            for (var i = 0; i < 20; i++) view[i] = i * 3;

            // Slice with odd offset and size to hit unaligned head/tail
            var sliced = sab.slice(3, 14);
            var result = new Uint8Array(sliced);
        "#,
        ),
        TestAction::assert("result.length === 11"),
        TestAction::assert("result[0] === 9"),
        TestAction::assert("result[10] === 39"),
    ]);
}

/// Tests TypedArray.set from a SharedArrayBuffer-backed array to a regular
/// ArrayBuffer-backed array, triggering `batched_copy_atomic_to_bytes`.
#[test]
fn shared_to_regular_typed_array_set() {
    run_test_actions([
        TestAction::run(
            r#"
            var sab = new SharedArrayBuffer(16);
            var src = new Uint8Array(sab);
            for (var i = 0; i < 16; i++) src[i] = 100 + i;

            var ab = new ArrayBuffer(16);
            var dest = new Uint8Array(ab);
            dest.set(src);
        "#,
        ),
        TestAction::assert("dest[0] === 100"),
        TestAction::assert("dest[7] === 107"),
        TestAction::assert("dest[15] === 115"),
    ]);
}

/// Tests TypedArray.set from a regular ArrayBuffer-backed array to a
/// SharedArrayBuffer-backed array, triggering `batched_copy_bytes_to_atomic`.
#[test]
fn regular_to_shared_typed_array_set() {
    run_test_actions([
        TestAction::run(
            r#"
            var ab = new ArrayBuffer(16);
            var src = new Uint8Array(ab);
            for (var i = 0; i < 16; i++) src[i] = 200 + i;

            var sab = new SharedArrayBuffer(16);
            var dest = new Uint8Array(sab);
            dest.set(src);
        "#,
        ),
        TestAction::assert("dest[0] === 200"),
        TestAction::assert("dest[7] === 207"),
        TestAction::assert("dest[15] === 215"),
    ]);
}

/// Tests forward `copyWithin` on a SharedArrayBuffer-backed typed array,
/// triggering `copy_shared_to_shared` via `memmove`.
#[test]
fn shared_typed_array_copy_within() {
    run_test_actions([
        TestAction::run(
            r#"
            var sab = new SharedArrayBuffer(16);
            var arr = new Uint8Array(sab);
            for (var i = 0; i < 16; i++) arr[i] = i + 1;

            // Forward copy: copies bytes 4..12 to offset 0
            arr.copyWithin(0, 4, 12);
        "#,
        ),
        TestAction::assert("arr[0] === 5"),
        TestAction::assert("arr[7] === 12"),
        TestAction::assert("arr[8] === 9"),
    ]);
}

/// Tests backward `copyWithin` on a SharedArrayBuffer-backed typed array,
/// triggering `copy_shared_to_shared_backwards` when source and dest overlap.
#[test]
fn shared_typed_array_copy_within_backward() {
    run_test_actions([
        TestAction::run(
            r#"
            var sab = new SharedArrayBuffer(16);
            var arr = new Uint8Array(sab);
            for (var i = 0; i < 16; i++) arr[i] = i + 1;

            // Backward copy: copies bytes 0..8 to offset 4 (overlapping)
            arr.copyWithin(4, 0, 8);
        "#,
        ),
        TestAction::assert("arr[0] === 1"),
        TestAction::assert("arr[3] === 4"),
        TestAction::assert("arr[4] === 1"),
        TestAction::assert("arr[11] === 8"),
        TestAction::assert("arr[12] === 13"),
    ]);
}

/// Tests zero-length slice to exercise the `count == 0` early return.
#[test]
fn shared_array_buffer_slice_empty() {
    run_test_actions([
        TestAction::run(
            r#"
            var sab = new SharedArrayBuffer(16);
            var view = new Uint8Array(sab);
            for (var i = 0; i < 16; i++) view[i] = i + 1;
            var sliced = sab.slice(5, 5);
            var result = new Uint8Array(sliced);
        "#,
        ),
        TestAction::assert("result.length === 0"),
    ]);
}
