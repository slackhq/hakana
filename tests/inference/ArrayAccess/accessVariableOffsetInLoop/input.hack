function foo(): vec<string> {
    $out = vec[];
    $stack = 0;

    while (rand(0, 1)) {
        if (rand(0, 1)) {
            $stack++;
        }
        
        if (rand(0, 1)) {
            if ($stack) {
                $stack--;
                $out[] = "a";
            }
        }
    }

    return $out;
}