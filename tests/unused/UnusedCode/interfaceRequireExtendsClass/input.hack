abstract class Node {}

interface HasFooNode {
    public function foo(): void;
}

trait HasFooNodeTrait implements HasFooNode {
    public function foo(): void {}
}

class FooNode extends Node {
    use HasFooNodeTrait;
    
    public function foo(): void {}
}

function takes_node(Node $node): void {
    if ($node is HasFooNode) {
        $node->foo();
    }
}

<<__EntryPoint>>
function main(): void {
    takes_node(new FooNode());
}