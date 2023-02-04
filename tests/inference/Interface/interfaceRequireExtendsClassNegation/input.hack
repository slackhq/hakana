abstract class Node {}

interface HasFooNode {
    require extends Node;

    public static function foo(): void;
}

class FooNode extends Node implements HasFooNode {
    public static function foo(): void {}
}

function takes_node(Node $node): void {
    if ($node is HasFooNode) {
        // do nothing
    }

    echo $node;
}