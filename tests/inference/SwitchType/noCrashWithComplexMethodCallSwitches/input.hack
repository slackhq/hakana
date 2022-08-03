function fromFoo(): int {
   switch (true) {
       case (rand(0, 1) && rand(0, 2)):
       case (rand(0, 3) && rand(0, 4)):
       case (rand(0, 5) && rand(0, 6)):
       case (rand(0, 7) && rand(0, 8)):
       case (rand(0, 7) && rand(0, 8)):
       case (rand(0, 7) && rand(0, 8)):
       case (rand(0, 7) && rand(0, 8)):
           return 1;
       default:
           return 0;
   }
                   }