min1 * 0 = 0
min1 0 * = 0
min1 a b = (+ 1 (min1 (- a 1) (- b 1)))

min2 a b = switch a {
  0: 0
  _: switch b {
    0: 0
    _: (+ 1 (min2 a-1 b-1))
  }
}

map f xs = match xs {
  List/Cons: (List/Cons (f xs.head) (map f xs.tail))
  List/Nil: List/Nil
}

vals = [(5, 10) (10, 5) (0, 12) (12 0) (0 0) (6 6)]

main =
  let f1 = @x let (a, b) = x; (min1 a b)
  let f2 = @x let (a, b) = x; (min2 a b)
  let a = [f1 f2]
  (map @a (map a vals) a)
 