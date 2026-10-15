FROM alpine:latest
RUN mkdir /app && echo one > /app/a.txt && echo two > /app/b.txt
COPY note.txt /app/note.txt
RUN rm /app/b.txt
