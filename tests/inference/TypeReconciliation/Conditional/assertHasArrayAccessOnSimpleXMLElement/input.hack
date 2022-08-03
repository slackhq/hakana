function getBar(SimpleXMLElement $e, string $s) : void {
    if (isset($e[$s])) {
        echo (string) $e[$s];
    }

    if (isset($e['foo'])) {
        echo (string) $e['foo'];
    }

    if (isset($e->bar)) {}
}