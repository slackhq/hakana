function foo(int $counter) : void {
    foreach (vec[1, 2, 3] as $_) {
        $counter = $counter + 1;
        echo $counter;
        echo rand(0, 1) ? 1 : 0;
    }
}