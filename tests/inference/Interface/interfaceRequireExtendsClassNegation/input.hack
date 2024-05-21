abstract class Node {
    public static function bar(): void {}
}

interface HasFooNode {
    require extends Node;

    public static function foo(): void;
}

final class FooNode extends Node implements HasFooNode {
    public static function foo(): void {}
}

function takes_node(Node $node): void {
    if ($node is HasFooNode) {
        $node::foo();
        $node::bar();
    }

    echo $node;
}