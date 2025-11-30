class Base {
    public function greet(): string {
        return "Hello";
    }
}

class Child extends Base {
    <<__Override>>
    public function greet(): string {
        return parent::greet() . " World";
    }
}

function useBase(Base $b): string {
    return $b->greet();
}

function test(): void {
    $base = new Base();
    $child = new Child();
    useBase($base);
    useBase($child);
}
