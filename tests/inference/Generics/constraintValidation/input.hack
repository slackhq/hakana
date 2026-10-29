final class Ref<T> {
    public function __construct(public T $value) {}
}

function identity<T>(T $value): T {
    return $value;
}

function take_shape(shape('unknown' => int, 'value' => int) $value): void {}

function invalid_shape(string $json): void {
    take_shape(identity(shape('unknown' => json_decode($json, true), 'value' => 'bad')));
}

function invalid_append(): void {
    $ref = new Ref<vec<int>>(vec[]);
    $ref->value[] = 'bad';
}

function invalid_keyed_write(string $key): void {
    $ref = new Ref<dict<string, int>>(dict[]);
    $ref->value[$key] = 'bad';
}

type Binding = shape('name' => string);
function take_typename(typename<int> $name): void {}

function invalid_typename(): void {
    take_typename(Binding::class);
}
