function takes_falsable(Traversable<string> $x) {
    if ($x) {
        \var_dump('not always truthy, because an empty array is a traversable');
    }
}