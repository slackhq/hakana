final class Box<T> {}
final class Output<+T> {}
type BoxAlias<T> = Box<T>;

function identity<T>(T $value): T {
    return $value;
}

function numeric<T as num>(T $value): T {
    return $value;
}

function make_box<T>(): BoxAlias<T> {
    invariant_violation('stub');
}

function output<T>(T $value): Output<T> {
    invariant_violation('stub');
}

function numeric_output<T as num>(Output<T> $value): Output<T> {
    return $value;
}

function callback<T>((function(T): void) $callback): (function(T): void) {
    return $callback;
}

function accepts_box(Box<int> $box): void {}
function accepts_boxes(vec<Box<int>> $boxes): void {}
function accepts_callback((function(int): void) $callback): void {}

function factories(): void {
    accepts_box(make_box());
    accepts_boxes(vec[make_box(), make_box()]);
}

function preserves_narrow_type(int $value): int {
    return numeric(identity($value));
}

function preserves_covariant_type(int $value): Output<int> {
    return numeric_output(output($value));
}

function reverses_callback_constraint((function(num): void) $f): void {
    accepts_callback(callback($f));
}

function independent_calls(int $i, string $s): (int, string) {
    return tuple(identity($i), identity($s));
}

function preserves_explicit_type(int $value): num {
    return numeric(identity<num>($value));
}

function returned_factory(): Box<int> {
    return make_box();
}

async function async_box<T>(): Awaitable<Box<T>> {
    await \HH\Asio\usleep(1);
    invariant_violation('stub');
}

async function awaited_factory(): Awaitable<Box<int>> {
    accepts_box(await async_box());
    return await async_box();
}

function make_class<T>(): classname<T> {
    invariant_violation('stub');
}

function takes_class(classname<Box<int>> $class): void {}

function class_factory(): classname<Box<int>> {
    takes_class(make_class());
    return make_class();
}
