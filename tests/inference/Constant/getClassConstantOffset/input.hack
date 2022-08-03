class C {
    const dict<int, string> A = dict[0 => "string" ];
    const string B = self::A[0];

    public function foo(): string {
        return self::B;
    }
}