<<__Sealed(First::class, Second::class)>>
interface Value {}

final class First implements Value {}

final class Second implements Value {
    public function get(): int {
        return 1;
    }
}

function read_value(Value $value): int {
    if ($value is First) {
        return 0;
    }
    return $value->get();
}
