function foo(dict<int, string> $test) : void {
    foreach($test as $key => $_testValue) {
        echo $key;
    }
}