abstract class Node {}
class FooNode extends Node {}

function foo<T as ?Node>(T $t): FooNode {
    return $t as FooNode;
}