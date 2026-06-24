function foo() : void {
    $link = null;

    while (rand(0, 1)) {
        if (rand(0, 1) !== 0) {
            $link = "a";
            continue;
        }

        if (rand(0, 1) !== 0) {
            if ($link === null) {
               return;
            }

            continue;
        }
    }
}
