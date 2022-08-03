$a = false;

foreach (vec["a", "b", "c"] as $tag) {
    if (!$a) {
        $a = true;
        break;
    }
}