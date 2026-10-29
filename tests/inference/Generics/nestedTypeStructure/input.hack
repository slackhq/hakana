type Binding = shape('name' => string);

function read_structure(TypeStructure<mixed> $structure): void {}

function nested(): void {
    read_structure(HH\type_structure_for_alias(Binding::class));
}

function read_typed_structure<T>(TypeStructure<T> $structure, mixed $value): T {
    throw new Exception();
}

final class Definition {
    const type TBinding = shape('binding' => Binding);

    public static function read(mixed $value): self::TBinding {
        return read_typed_structure(type_structure(self::class, 'TBinding'), $value);
    }
}
