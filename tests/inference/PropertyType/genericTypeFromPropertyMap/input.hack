function foo(DOMElement $e) : ?DOMAttr {
    return $e->attributes->item(0);
}