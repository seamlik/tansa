import static java.net.NetworkInterface.networkInterfaces;
import static java.util.stream.Collectors.joining;

boolean hasIpv6(NetworkInterface inter) {
  return inter.inetAddresses().anyMatch(address -> address instanceof Inet6Address);
}

boolean isCandidate(NetworkInterface inter) {
  try {
    return inter.isUp() && inter.supportsMulticast() && hasIpv6(inter);
  } catch (SocketException e) {
    throw new IllegalStateException(e);
  }
}

String getIndex(NetworkInterface inter) {
  return Integer.toString(inter.getIndex());
}

var indexes = networkInterfaces().filter(i -> isCandidate(i)).map(i -> getIndex(i)).collect(joining(" "));
System.out.println(indexes);
