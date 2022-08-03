function foo() : void {
    try {
        $a = 0;
    } catch (Exception $e) {
        echo isset($a) ? $a : 1;
    }
}