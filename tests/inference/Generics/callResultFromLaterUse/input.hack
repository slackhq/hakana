final class Box<T> {}

function make_box<T>(): Box<T> {
    return new Box();
}

function accept_int(Box<int> $box): void {}
function accept_string(Box<string> $box): void {}

function inferred_from_later_use(): void {
    $box = make_box();
    accept_int($box);
}

function conflicting_uses(): void {
    $box = make_box();
    accept_int($box);
    accept_string($box);
}
