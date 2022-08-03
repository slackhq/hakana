class U {
    public function takes(U $u) : bool {
        return true;
    }
}

function bar(?U $a, ?U $b) : void {
    if ($a === null
        || ($b !== null && $a->takes($b))
        || $b === null
    ) {}
}