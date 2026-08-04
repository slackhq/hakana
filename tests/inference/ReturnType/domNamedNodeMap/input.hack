function foo(DOMNamedNodeMap<DOMAttr> $map) {
    foreach ($map as $item) {
        if ($item is DOMAttr) {
            var_dump($item);
        }
    }
}
