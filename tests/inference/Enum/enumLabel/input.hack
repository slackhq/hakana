enum class E: int {
  int A = 42;
  int B = 42;
}

function full_print(\HH\EnumClass\Label<E, int> $label): void {
    echo E::nameOf($label) . " ";
    echo E::valueOf($label) . "\n";
}

function get_value<T>(\HH\EnumClass\Label<E, T> $label): T {
    return E::valueOf($label);
}

function foo(): int {
    full_print(E#A);
    full_print(#B);
    return get_value(E#A);
}