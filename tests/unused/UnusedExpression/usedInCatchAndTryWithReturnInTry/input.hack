function foo() : ?string {
    $a = null;

    try {
        $a = "hello";
        echo $a;
    } catch (Exception $e) {
        return $a;
    }

    return $a;
}

function dangerous() : string {
    if (rand(0, 1) !== 0) {
        throw new \Exception("bad");
    }
    return "hello";
}
