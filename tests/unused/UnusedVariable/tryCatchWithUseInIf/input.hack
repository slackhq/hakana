function example_string() : string {
    if (rand(0, 1) > 0) {
        return "value";
    }
    throw new Exception("fail");
}

function main() : void {
    try {
        $s = example_string();
        if (!$s) {
            echo "Failed to get string\n";
        }
    } catch (Exception $e) {
        $s = "fallback";
    }
    printf("s is %s\n", $s);
}