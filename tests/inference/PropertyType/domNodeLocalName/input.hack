function foo(DOMNode $node): string {
    return $node->localName ?? $node->nodeName;
}
