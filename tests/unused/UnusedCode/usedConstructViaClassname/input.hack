<<__ConsistentConstruct>>
abstract class Base {
    public function __construct(string $s): void {
        echo $s;
    }
}

final class Concrete extends Base {
    public function __construct(string $s): void {
        echo $s;
    }
}

function newWithClassname(classname<Base> $c) {
    new $c("a");
}

<<__EntryPoint>>
function main(): void {
    newWithClassname(Concrete::class);
}