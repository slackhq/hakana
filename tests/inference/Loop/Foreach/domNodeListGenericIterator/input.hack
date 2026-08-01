function enqueue_dom_children(
	DOMNodeList<DOMNode> $nodes,
	SplQueue<DOMNode> $queue,
): void {
	foreach ($nodes as $child) {
		$queue->enqueue($child);
	}
}
