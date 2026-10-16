type data_type = shape('id' => int, 'names' => vec<string>);
type names_type = vec<string>;
type maybe_int_type = ?int;

<<Hakana\TypeAssertion('$value', 'T')>>
function assert_type<T>(mixed $value, typename<T> $type): T {
    throw new Exception();
}

function test(data_type $data, vec<string> $names, int $id): void {
    assert_type($data, data_type::class);
    assert_type($names, names_type::class);
    assert_type($id, data_type::class);
    assert_type($data, names_type::class);
    assert_type($id, maybe_int_type::class);
}
