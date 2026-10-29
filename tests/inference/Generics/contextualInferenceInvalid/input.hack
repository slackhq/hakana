final class Box<T> {}

function identity<T>(T $value): T {
    return $value;
}

function make_box<T>(T $value): Box<T> {
    invariant_violation('stub');
}

function takes_int(int $value): void {}
function takes_box(Box<int> $box): void {}

function incompatible_context(string $value): void {
    takes_int(identity($value));
    takes_box(make_box($value));
    takes_int(identity<string>($value));
}
