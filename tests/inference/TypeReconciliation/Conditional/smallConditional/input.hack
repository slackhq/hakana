class A {
    public vec<mixed> $parts = vec[];
}

class FuncCall {
    public ?A $name;
    public dict<arraykey, string> $args = dict[];
}

function barr(FuncCall $function) : void {
    if (!$function->name is A) {
        return;
    }

    if ($function->name->parts === vec["function_exists"]
        && isset($function->args[0])
    ) {
        // do something
    } else if ($function->name->parts === vec["class_exists"]
        && isset($function->args[0])
    ) {
        // do something else
    }
}