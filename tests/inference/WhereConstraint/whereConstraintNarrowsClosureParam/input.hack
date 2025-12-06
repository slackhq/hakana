abstract class Node {
}

interface HasItem<+T> {
    public function getItem(): T;
}

final class ListItem<T> extends Node implements HasItem<T> {
    public function __construct(private T $item) {}
    <<__Override>>
    public function getItem(): T {
        return $this->item;
    }
}

final class Container<+TItem as Node> extends Node {
    public function __construct(private vec<TItem> $items) {}

    public function getItems(): vec<TItem> {
        return $this->items;
    }

    // The where constraint should narrow TItem to HasItem<T> for the closure
    public function testVecMap<T>(): vec<T> where TItem as HasItem<T> {
        return Vec\map($this->getItems(), $item ==> $item->getItem());
    }
}
