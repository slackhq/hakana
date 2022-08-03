function foo() : void {
    do {
        try {
            do {
                $count = rand(0, 10);
            } while ($count === 5);
        } catch (Exception $e) {}
    } while (rand(0, 1));
}