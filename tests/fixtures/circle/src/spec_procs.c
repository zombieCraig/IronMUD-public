/* Test fixture for parse_puff_quotes(). Stripped-down puff() body with
 * two literal do_say quotes; the importer should pull them into the
 * @say_random template's args field.
 */

SPECIAL(puff)
{
  char actbuf[MAX_INPUT_LENGTH];

  if (cmd)
    return (FALSE);

  switch (rand_number(0, 60)) {
  case 0:
    do_say(ch, strcpy(actbuf, "My god!  It's full of stars!"), 0, 0);
    return (TRUE);
  case 1:
    do_say(ch, strcpy(actbuf, "How'd all those fish get up here?"), 0, 0);
    return (TRUE);
  default:
    return (FALSE);
  }
}

SPECIAL(other) {
  /* should NOT contribute to puff's quote list */
  do_say(ch, "not puff", 0, 0);
}
