abstract class Node {
	public function getText(): string {
		return "x";
	}
}

final class NodeList<T as Node> extends Node {
	public function __construct(private vec<T> $items) {}

	public function getChildren(): vec<T> {
		return $this->items;
	}
}

// `as NodeList<_>` should infer the wildcard as the template's `as`
// constraint (Node), not arraykey
function get_first_text(Node $n): string {
	$list = $n as NodeList<_>;
	$children = $list->getChildren();
	return $children[0]->getText();
}
