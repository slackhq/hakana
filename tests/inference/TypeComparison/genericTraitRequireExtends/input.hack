abstract class Node<T> {}

trait Leaf<T> {
	require extends Node<T>;
}

abstract class Field extends Node<Field> {}

abstract class HasField extends Field {
	use Leaf<Field>;
}

final class HasFieldValue extends HasField {}

function checkNode(Node<Field> $node): void {
	if ($node is HasFieldValue) {
		// This should be valid - HasFieldValue extends Node<Field> through inheritance chain
	}
}
