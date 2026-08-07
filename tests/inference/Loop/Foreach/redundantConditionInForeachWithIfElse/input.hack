$a = false;

foreach (vec["a", "b", "c"] as $tag) {
    if (!$a) {
        if (rand(0, 1) !== 0) {
            $a = true;
            break;
        } else {
            $a = true;
            break;
        }
    }
}
