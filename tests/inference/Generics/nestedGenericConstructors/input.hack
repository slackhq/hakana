/* HAKANA_FIXME[InterfaceSingleImplementor] */
interface Comparable<T> {
    public function equals(T $other): bool;
}

final class Value implements Comparable<Value> {
    <<__Override>>
    public function equals(Value $other): bool {
        return true;
    }
}

abstract class Node<T> {
    abstract public function getValue(): T;
    abstract public function getChildren(): vec<Node<T>>;
}

final class TestNode<T> extends Node<T> {
    public function __construct(public T $value, public vec<Node<T>> $children) {}

    <<__Override>>
    public function getValue(): T {
        return $this->value;
    }

    <<__Override>>
    public function getChildren(): vec<Node<T>> {
        return $this->children;
    }
}

function equal<T as Comparable<T>>(Node<T> $a, Node<T> $b): bool {
    if (!$a->getValue()->equals($b->getValue())) {
        return false;
    }
    foreach ($a->getChildren() as $i => $child) {
        if (!equal($child, $b->getChildren()[$i])) {
            return false;
        }
    }
    return true;
}

function nested(): bool {
    $a = new TestNode(new Value(), vec[new TestNode(new Value(), vec[])]);
    $b = new TestNode(new Value(), vec[new TestNode(new Value(), vec[])]);
    return equal($a, $b);
}
