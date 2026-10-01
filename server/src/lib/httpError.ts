/** Error carrying the HTTP status the API error handler should answer with. */
export const httpError = (statusCode: number, message: string) =>
  Object.assign(new Error(message), { statusCode });
