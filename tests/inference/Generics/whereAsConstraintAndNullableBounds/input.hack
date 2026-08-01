abstract class WcNode {}

final class WcToken extends WcNode {}

final class WcLItem<+T as ?WcNode> extends WcNode {
	public function __construct(private T $item, private ?WcToken $sep) {}

	public function getItem(): T {
		return $this->item;
	}

	// hh: filter_nulls solves the value as T & nonnull, which is
	// contained in WcNode when T as ?WcNode
	public function getChildren(): dict<string, WcNode> {
		return \HH\Lib\Dict\filter_nulls(dict[
			'item' => $this->item,
			'separator' => $this->sep,
		]);
	}
}

final class WcNList<Titem as WcNode> extends WcNode {
	public function __construct(private vec<Titem> $items) {}

	public function getChildrenOfItemsOfType<T as ?WcNode>(
		classname<T> $what,
	): vec<T> where Titem as WcLItem<T> {
		return vec[];
	}
}

final class WcSpecificNode extends WcNode {}

// `where Titem as WcLItem<T>` is a constraint, not an equality;
// T can be solved wider from the argument (hh accepts this)
function f(WcNList<WcLItem<WcSpecificNode>> $list): vec<WcNode> {
	return $list->getChildrenOfItemsOfType(WcNode::class);
}
