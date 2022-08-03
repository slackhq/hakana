function foo(string $b) : void {
    switch ($b){
        case "foo":
            $a = null;
            foreach (vec[1,2,3] as $f){
                if ($f == 2) {
                    $a = $f;
                    break;
                }
            }
            echo $a;
    }
}