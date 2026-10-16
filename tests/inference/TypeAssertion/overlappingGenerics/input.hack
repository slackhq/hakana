<<Hakana\TypeAssertion('$value', 'T')>>
function verify<reify T>(mixed $value): ?T {
    return $value ?as T;
}

final class Cache<T> {
    public function check<reify U>(?T $value): void {
        if ($value is nonnull) {
            verify<U>($value);
            $value ?as U;
        }
    }
}

function overlapping_bounds<T as int, reify U as num>(T $value): void {
    verify<U>($value);
    $value ?as U;
}

function invariant_collection(): vec<string> {
    $values = new \HH\Lib\Ref(vec[]);
    $append = () ==> {
        $values->value[] = 'first';
    };
    $append();
    return $values->value;
}
