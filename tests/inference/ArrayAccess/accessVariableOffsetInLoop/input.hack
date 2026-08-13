function foo(): vec<string> {
    $out = vec[];
    $stack = 0;

    while (rand(0, 1)) {
        if (rand(0, 1) !== 0) {
            $stack++;
        }
        
        if (rand(0, 1) !== 0) {
            if ($stack !== 0) {
                $stack--;
                $out[] = "a";
            }
        }
    }

    return $out;
}
