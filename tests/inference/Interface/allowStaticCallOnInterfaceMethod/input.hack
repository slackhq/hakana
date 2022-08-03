interface IFoo {
    public static function doFoo() : void;
}

function bar(IFoo $i) : void {
    $i::doFoo();
}