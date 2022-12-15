function foo(dict<string, string> $array) : void {
    $c = 0;

    if ($array["a"] === "a") {
        foreach (vec[rand(0, 1), rand(0, 1)] as $i) {
            if ($array["b"] === "c") {}
            $c++;
        }
    }
}