<<Hakana\TypeAssertion('$value', 'T')>>
function verify<reify T>(mixed $value): ?T {
    return $value ?as T;
}

function nonempty_vectors(): void {
    verify<vec<string>>(vec[1]);
    vec[1] ?as vec<string>;
    verify<Traversable<string>>(vec[1]);
    vec[1] ?as Traversable<string>;
}

function nonempty_dicts(): void {
    verify<dict<string, string>>(dict['id' => 1]);
    dict['id' => 1] ?as dict<string, string>;
    verify<dict<int, int>>(dict['id' => 1]);
    dict['id' => 1] ?as dict<int, int>;
}

function required_shape_field(shape('id' => int) $value): void {
    verify<shape('id' => string)>($value);
    $value ?as shape('id' => string);
}

function disjoint_bounds<T as int, reify U as string>(T $value): void {
    verify<U>($value);
    $value ?as U;
}

function incompatible_tuples(): void {
    verify<(int, int)>(tuple(1, 'a'));
    tuple(1, 'a') ?as (int, int);
    verify<(int, string, int)>(tuple(1, 'a'));
    tuple(1, 'a') ?as (int, string, int);
}

function incompatible_interface_elements(): void {
    verify<Traversable<string>>(vec['a', 1]);
    vec['a', 1] ?as Traversable<string>;
    verify<Traversable<string>>(dict['a' => 'b', 'c' => 1]);
    dict['a' => 'b', 'c' => 1] ?as Traversable<string>;
}
