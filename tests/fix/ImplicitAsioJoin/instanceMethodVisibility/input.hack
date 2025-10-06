namespace {
    abstract class A {
        protected async function prot_async_method(): Awaitable<int> {
            await \HH\Asio\later();
            return 42;
        }

        public function sync_method(): int {
            return \HH\Asio\join($this->prot_async_method());
        }
    }

    final class B extends A {
        public async function foo(): Awaitable<int> {
            await \HH\Asio\later();
            return $this->sync_method() + C::sync_method();
        }

        public function sync_foo(): int {
            return \HH\Asio\join($this->foo());
        }
    }

    final class C {
        private static async function priv_async_method(): Awaitable<int> {
            await \HH\Asio\later();
            return 42;
        }

        public static function sync_method(): int {
            return \HH\Asio\join(self::priv_async_method());
        }

        public static async function sync_caller(): Awaitable<int> {
            await \HH\Asio\later();
            return rand() + self::sync_method();
        }
    }

    async function from_function(): Awaitable<int> {
        await \HH\Asio\later();

        $b = new B();

        return $b->sync_method() + $b->sync_foo() + C::sync_method();
    }
}
