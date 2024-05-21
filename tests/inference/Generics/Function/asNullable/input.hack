abstract class Node {}
final class FooNode extends Node {}

function foo<T as ?Node>(T $t): FooNode {
    return $t as FooNode;
}