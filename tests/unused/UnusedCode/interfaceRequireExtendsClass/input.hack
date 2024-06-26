abstract class Node {}

interface IHasDefault {
    public function isDefault(mixed $v): bool;
    public static function getDefault(): mixed;
}

trait HasDefault implements IHasDefault {
    public function isDefault(mixed $v): bool {
        return static::getDefault() == $v;
    }
}

final class FooNode extends Node {
    use HasDefault;
    
    public static function getDefault(): mixed {
        return '';
    }
}

function takes_node(Node $node): void {
    if ($node is IHasDefault && $node->isDefault('')) {
        $node::getDefault();
    }
}

<<__EntryPoint>>
function main(): void {
    takes_node(new FooNode());
}