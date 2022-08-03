class X33{
    public static function main(): void {
        echo SELF::class . "\n";  // Class or interface SELF does not exist
    }
}
X33::main();