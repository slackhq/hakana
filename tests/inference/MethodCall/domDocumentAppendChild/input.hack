$doc = new DOMDocument("1.0");
$node = $doc->createElement("foo");
if ($node is DOMElement) {
    $newnode = $doc->appendChild($node);
    $newnode->setAttribute("bar", "baz");
}