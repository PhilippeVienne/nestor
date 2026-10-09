FROM alpine:3.20
RUN apk add --no-cache dnsmasq
CMD ["dnsmasq", "--keep-in-foreground", "--conf-file=/etc/dnsmasq.conf"]
