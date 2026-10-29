abstract class Base<T as arraykey> {
    public function __construct(public keyset<T> $values) {}
}

final class Ints extends Base<int> {}

function make_ints(vec<int> $values): Ints {
    return new Ints(keyset($values));
}

function make_strings(vec<string> $values): Ints {
    return new Ints(keyset($values));
}
