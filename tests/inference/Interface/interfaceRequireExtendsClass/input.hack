abstract class Node {}
interface INode {
    require extends Node;
}
final class FooNode extends Node implements INode {}

function foo(INode $node): Node {
    return $node;
}

foo(new FooNode()); 